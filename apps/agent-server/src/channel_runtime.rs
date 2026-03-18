use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agent_store::{
    ChannelMessageReceipt, ChannelSessionBinding, ExternalConversationKey, FeishuMessageTarget,
};
use channel_registry::{ChannelProfile, ChannelTransport};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{task::JoinHandle, time::MissedTickBehavior};
use tokio_tungstenite::{connect_async, tungstenite::Message as WebSocketMessage};

use crate::{
    runtime_worker::CurrentTurnSnapshot,
    session_manager::{RuntimeWorkerError, SessionManagerHandle, read_lock},
    sse::SsePayload,
    state::AppState,
};

const FEISHU_EVENT_KIND: &str = "im.message.receive_v1";
const FEISHU_WS_ENDPOINT_URI: &str = "/callback/ws/endpoint";
const FEISHU_P2P_CHAT_QUERY_URI: &str = "/open-apis/im/v1/chat_p2p/batch_query";
const FEISHU_CARDKIT_CARDS_URI: &str = "/open-apis/cardkit/v1/cards";
const FEISHU_CARDKIT_CARD_UPDATE_URI: &str = "/open-apis/cardkit/v1/cards/{card_id}";
const FEISHU_CARDKIT_CARD_SETTINGS_URI: &str = "/open-apis/cardkit/v1/cards/{card_id}/settings";
const FEISHU_CARDKIT_CARD_ELEMENT_CONTENT_URI: &str =
    "/open-apis/cardkit/v1/cards/{card_id}/elements/{element_id}/content";
const FEISHU_MESSAGE_REACTIONS_URI: &str = "/open-apis/im/v1/messages/{message_id}/reactions";
const FEISHU_MESSAGES_URI: &str = "/open-apis/im/v1/messages";
const FEISHU_PROCESSING_EMOJI_TYPE: &str = "Typing";
const FEISHU_CARD_UPDATE_INTERVAL_MS: u64 = 700;
const FEISHU_CARDKIT_STREAMING_ELEMENT_ID: &str = "streaming_content";
const FEISHU_HEADER_TYPE: &str = "type";
const FEISHU_HEADER_MESSAGE_ID: &str = "message_id";
const FEISHU_HEADER_SUM: &str = "sum";
const FEISHU_HEADER_SEQ: &str = "seq";
const FEISHU_HEADER_BIZ_RT: &str = "biz_rt";
const FEISHU_HEADER_SERVICE_ID: &str = "service_id";
const FEISHU_MESSAGE_TYPE_EVENT: &str = "event";
const FEISHU_MESSAGE_TYPE_PING: &str = "ping";
const FEISHU_MESSAGE_TYPE_PONG: &str = "pong";
const FEISHU_FRAME_TYPE_CONTROL: i32 = 0;
const FEISHU_FRAME_TYPE_DATA: i32 = 1;

#[derive(Clone)]
struct FeishuRuntimeDeps {
    store: Arc<agent_store::AiaStore>,
    session_manager: SessionManagerHandle,
    broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
}

struct RunningFeishuWorker {
    fingerprint: String,
    handle: JoinHandle<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeWorkerState {
    fingerprint: String,
    finished: bool,
}

pub struct FeishuRuntimeSupervisor {
    deps: FeishuRuntimeDeps,
    workers: HashMap<String, RunningFeishuWorker>,
}

impl FeishuRuntimeSupervisor {
    pub fn new(
        store: Arc<agent_store::AiaStore>,
        session_manager: SessionManagerHandle,
        broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
    ) -> Self {
        Self {
            deps: FeishuRuntimeDeps { store, session_manager, broadcast_tx },
            workers: HashMap::new(),
        }
    }

    fn spawn_worker(&self, profile: ChannelProfile) -> RunningFeishuWorker {
        let fingerprint = profile_fingerprint(&profile);
        let deps = self.deps.clone();
        let handle = tokio::spawn(async move {
            run_feishu_long_connection(profile, deps).await;
        });
        RunningFeishuWorker { fingerprint, handle }
    }

    async fn sync(&mut self, profiles: Vec<ChannelProfile>) {
        let existing = self
            .workers
            .iter()
            .map(|(profile_id, worker)| {
                (
                    profile_id.clone(),
                    RuntimeWorkerState {
                        fingerprint: worker.fingerprint.clone(),
                        finished: worker.handle.is_finished(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let (stop_ids, start_profiles) = reconcile_runtime_workers(&existing, &profiles);

        for profile_id in stop_ids {
            if let Some(worker) = self.workers.remove(&profile_id) {
                worker.handle.abort();
            }
        }

        for profile in start_profiles {
            let profile_id = profile.id.clone();
            if let Some(worker) = self.workers.remove(&profile_id) {
                worker.handle.abort();
            }
            self.workers.insert(profile_id, self.spawn_worker(profile));
        }
    }
}

impl Drop for FeishuRuntimeSupervisor {
    fn drop(&mut self) {
        for worker in self.workers.drain().map(|(_, worker)| worker) {
            worker.handle.abort();
        }
    }
}

pub async fn sync_feishu_runtime(state: &AppState) -> Result<(), String> {
    let registry = read_lock(&state.channel_registry_snapshot).clone();
    let desired_profiles = registry
        .channels()
        .iter()
        .filter(|profile| profile.transport == ChannelTransport::Feishu && profile.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let mut runtime = state.feishu_runtime.lock().await;
    runtime.sync(desired_profiles).await;
    Ok(())
}

async fn run_feishu_long_connection(profile: ChannelProfile, deps: FeishuRuntimeDeps) {
    let mut policy = FeishuConnectionPolicy::default();
    let mut reconnect_attempts = 0_usize;
    loop {
        match fetch_websocket_endpoint(&profile).await {
            Ok(endpoint) => {
                policy.apply_server_config(endpoint.client_config.as_ref());
                reconnect_attempts = 0;
                if let Err(error) =
                    run_feishu_connection(&profile, &deps, &endpoint.url, &mut policy).await
                {
                    eprintln!("飞书长连接中断 profile_id={} error={error}", profile.id);
                }
            }
            Err(error) => {
                eprintln!("飞书长连接建连失败 profile_id={} error={error}", profile.id);
            }
        }
        reconnect_attempts = reconnect_attempts.saturating_add(1);
        if policy.should_stop_reconnecting(reconnect_attempts) {
            eprintln!("飞书长连接停止重连 profile_id={} attempts={reconnect_attempts}", profile.id);
            return;
        }
        if let Some(jitter) = policy.next_reconnect_jitter() {
            tokio::time::sleep(jitter).await;
        }
        tokio::time::sleep(policy.reconnect_interval).await;
    }
}

async fn run_feishu_connection(
    profile: &ChannelProfile,
    deps: &FeishuRuntimeDeps,
    url: &str,
    policy: &mut FeishuConnectionPolicy,
) -> Result<(), String> {
    let parsed_url = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
    let service_id = parsed_url
        .query_pairs()
        .find_map(|(key, value)| (key == FEISHU_HEADER_SERVICE_ID).then_some(value.into_owned()))
        .ok_or_else(|| "飞书长连接 endpoint 缺少 service_id".to_string())?
        .parse::<i32>()
        .map_err(|error| error.to_string())?;

    let (mut socket, _) = connect_async(url).await.map_err(|error| error.to_string())?;
    let mut pending_frames = PendingFrameBuffer::default();
    let mut ping_interval = tokio::time::interval(policy.ping_interval);
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ping_interval.tick().await;

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                let ping_frame = FeishuFrame::new_ping(service_id);
                socket
                    .send(WebSocketMessage::Binary(ping_frame.encode().into()))
                    .await
                    .map_err(|error| error.to_string())?;
            }
            message = socket.next() => {
                match message {
                    Some(Ok(WebSocketMessage::Binary(bytes))) => {
                        handle_feishu_frame(
                            profile,
                            deps,
                            &mut socket,
                            &mut pending_frames,
                            bytes.as_ref(),
                            policy,
                        ).await?;
                    }
                    Some(Ok(WebSocketMessage::Ping(payload))) => {
                        socket
                            .send(WebSocketMessage::Pong(payload))
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    Some(Ok(WebSocketMessage::Pong(_))) => {}
                    Some(Ok(WebSocketMessage::Text(_))) => {}
                    Some(Ok(WebSocketMessage::Frame(_))) => {}
                    Some(Ok(WebSocketMessage::Close(_))) => {
                        return Err("飞书长连接被服务端关闭".into());
                    }
                    Some(Err(error)) => return Err(error.to_string()),
                    None => return Err("飞书长连接已结束".into()),
                }
            }
        }
    }
}

async fn handle_feishu_frame(
    profile: &ChannelProfile,
    deps: &FeishuRuntimeDeps,
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    pending_frames: &mut PendingFrameBuffer,
    bytes: &[u8],
    policy: &mut FeishuConnectionPolicy,
) -> Result<(), String> {
    let frame = FeishuFrame::decode(bytes)?;
    let headers = FeishuHeaders::new(&frame.headers);

    if frame.method == FEISHU_FRAME_TYPE_CONTROL {
        if headers.get_string(FEISHU_HEADER_TYPE) == Some(FEISHU_MESSAGE_TYPE_PONG)
            && !frame.payload.is_empty()
            && let Ok(config) = serde_json::from_slice::<FeishuServerClientConfig>(&frame.payload)
        {
            policy.apply_server_config(Some(&config));
        }
        return Ok(());
    }

    if frame.method != FEISHU_FRAME_TYPE_DATA {
        return Ok(());
    }

    let payload = if let (Some(sum), Some(seq), Some(message_id)) = (
        headers.get_usize(FEISHU_HEADER_SUM),
        headers.get_usize(FEISHU_HEADER_SEQ),
        headers.get_string(FEISHU_HEADER_MESSAGE_ID),
    ) {
        if sum > 1 {
            match pending_frames.push(message_id.to_string(), sum, seq, frame.payload.clone()) {
                Some(payload) => payload,
                None => return Ok(()),
            }
        } else {
            frame.payload.clone()
        }
    } else {
        frame.payload.clone()
    };

    let started_at = tokio::time::Instant::now();
    let response = FeishuWsResponse::ok();
    let mut response_headers = frame.headers.clone();
    response_headers.push(FeishuHeader::new(
        FEISHU_HEADER_BIZ_RT,
        started_at.elapsed().as_millis().to_string(),
    ));
    let response_frame = frame.with_response_payload(response_headers, response.encode()?);
    socket
        .send(WebSocketMessage::Binary(response_frame.encode().into()))
        .await
        .map_err(|error| error.to_string())?;

    let payload = payload.clone();
    let profile = profile.clone();
    let deps = deps.clone();
    tokio::spawn(async move {
        if let Err(error) = handle_feishu_event_payload(&profile, &deps, &payload).await {
            eprintln!("飞书事件处理失败 profile_id={} error={error}", profile.id);
        }
    });
    Ok(())
}

async fn handle_feishu_event_payload(
    profile: &ChannelProfile,
    deps: &FeishuRuntimeDeps,
    payload: &[u8],
) -> Result<(), String> {
    let envelope: EventEnvelope =
        serde_json::from_slice(payload).map_err(|error| error.to_string())?;
    if envelope.header.event_type != FEISHU_EVENT_KIND {
        return Ok(());
    }
    handle_event(profile, deps, envelope.event).await
}

async fn fetch_websocket_endpoint(
    profile: &ChannelProfile,
) -> Result<FeishuWebsocketEndpoint, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!(
        "{}/{}",
        profile.config.base_url.trim_end_matches('/'),
        FEISHU_WS_ENDPOINT_URI.trim_start_matches('/'),
    );
    let response = client
        .post(url)
        .header("locale", "zh")
        .json(&json!({
            "AppID": profile.config.app_id,
            "AppSecret": profile.config.app_secret,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "获取飞书长连接 endpoint 失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload = response
        .json::<FeishuWebsocketEndpointResponse>()
        .await
        .map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("飞书长连接 endpoint 返回错误: {}", payload.msg));
    }
    let data = payload.data.ok_or_else(|| "飞书长连接 endpoint 缺少 data".to_string())?;
    if data.url.trim().is_empty() {
        return Err("飞书长连接 endpoint URL 为空".into());
    }
    Ok(data)
}

async fn handle_event(
    profile: &ChannelProfile,
    deps: &FeishuRuntimeDeps,
    event: EventBody,
) -> Result<(), String> {
    if event.sender.sender_type != "user" {
        return Ok(());
    }
    if event.message.message_type != "text" {
        return Ok(());
    }
    if profile.config.require_mention
        && event.message.chat_type == "group"
        && event.message.mentions.is_empty()
    {
        return Ok(());
    }

    let text = extract_text(&event.message.content)?;
    let prompt = normalize_text(&text, &event.message.mentions);
    if prompt.trim().is_empty() {
        return Ok(());
    }

    let conversation_key = resolve_conversation_key(profile, &event)?;
    let session_id = resolve_session_id(profile, deps, &conversation_key, &event).await?;
    let request_uuid_seed = event.message.message_id.clone();
    let receipt = ChannelMessageReceipt::new(
        "feishu",
        profile.id.clone(),
        event.message.message_id.clone(),
        session_id.clone(),
    );
    let first_seen = deps
        .store
        .record_channel_message_receipt_async(receipt)
        .await
        .map_err(|error| error.to_string())?;
    if !first_seen {
        return Ok(());
    }

    let reply_target = resolve_reply_target(profile, &event).await?;
    let turn_prompt = build_turn_prompt(&prompt, &event);
    spawn_turn_reply_job(
        profile.clone(),
        deps.clone(),
        session_id,
        turn_prompt,
        reply_target,
        request_uuid_seed,
    );
    Ok(())
}

fn spawn_turn_reply_job(
    profile: ChannelProfile,
    deps: FeishuRuntimeDeps,
    session_id: String,
    prompt: String,
    reply_target: FeishuMessageTarget,
    request_uuid_seed: String,
) {
    tokio::spawn(async move {
        let reaction_id = add_processing_reaction(&profile, &request_uuid_seed)
            .await
            .map_err(|error| {
                eprintln!(
                    "添加飞书处理中表情失败 profile_id={} message_id={} error={error}",
                    profile.id, request_uuid_seed
                );
                error
            })
            .ok();
        if let Err(error) = stream_turn_to_feishu_reply(
            &profile,
            &deps,
            &session_id,
            &prompt,
            &reply_target,
            &request_uuid_seed,
        )
        .await
        {
            eprintln!(
                "发送飞书异步流式回复失败 profile_id={} message_id={} error={error}",
                profile.id, request_uuid_seed
            );
        }

        if let Some(reaction_id) = reaction_id
            && let Err(error) =
                remove_message_reaction(&profile, &request_uuid_seed, &reaction_id).await
        {
            eprintln!(
                "移除飞书处理中表情失败 profile_id={} message_id={} reaction_id={} error={error}",
                profile.id, request_uuid_seed, reaction_id
            );
        }
    });
}

async fn stream_turn_to_feishu_reply(
    profile: &ChannelProfile,
    deps: &FeishuRuntimeDeps,
    session_id: &str,
    prompt: &str,
    reply_target: &FeishuMessageTarget,
    request_uuid_seed: &str,
) -> Result<(), String> {
    prepare_session_for_turn(&deps.session_manager, session_id).await?;

    let mut rx = deps.broadcast_tx.subscribe();
    let expected_turn_id = deps
        .session_manager
        .submit_turn(session_id.to_string(), prompt.to_string())
        .await
        .map_err(runtime_error_to_string)?;

    let mut controller =
        FeishuStreamingReplyController::new(prompt.to_string(), current_timestamp_ms());
    controller.reply_mode = create_feishu_streaming_reply_mode(
        profile,
        reply_target,
        &controller.state,
        request_uuid_seed,
    )
    .await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let received = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| "等待飞书流式回复超时".to_string())
            .and_then(|result| result.map_err(|error| error.to_string()))?;

        match received {
            SsePayload::CurrentTurnStarted { session_id: sid, current_turn }
                if sid == session_id && current_turn.turn_id == expected_turn_id =>
            {
                controller.update_from_snapshot(&current_turn);
                controller.flush(profile, false).await;
            }
            SsePayload::Status { session_id: sid, turn_id, status }
                if sid == session_id && turn_id == expected_turn_id =>
            {
                controller.update_status(status);
                controller.flush(profile, false).await;
            }
            SsePayload::Stream { session_id: sid, turn_id, event }
                if sid == session_id && turn_id == expected_turn_id =>
            {
                controller.apply_stream(&event);
                controller.flush(profile, false).await;
            }
            SsePayload::TurnCompleted { session_id: sid, turn_id, turn }
                if sid == session_id && turn_id == expected_turn_id =>
            {
                finalize_feishu_card_state(
                    &mut controller.state,
                    Some(extract_assistant_segments_from_turn(&turn)),
                    turn.failure_message.clone(),
                    turn.finished_at_ms,
                );

                if controller.flush(profile, true).await.is_none() {
                    let fallback = controller.state.final_text();
                    send_reply_message(profile, reply_target, &fallback, request_uuid_seed).await?;
                }
                return Ok(());
            }
            SsePayload::Error { session_id: sid, turn_id, message }
                if sid == session_id && turn_id.as_deref() == Some(expected_turn_id.as_str()) =>
            {
                finalize_feishu_card_state(
                    &mut controller.state,
                    None,
                    Some(if message.contains("already running") || message.contains("正在") {
                        "当前会话仍在处理中，请稍后再试。".into()
                    } else {
                        format!("处理消息失败：{message}")
                    }),
                    current_timestamp_ms(),
                );

                if controller.flush(profile, true).await.is_none() {
                    let fallback = controller.state.final_text();
                    send_reply_message(profile, reply_target, &fallback, request_uuid_seed).await?;
                }
                return Ok(());
            }
            _ => {}
        }
    }
}

struct FeishuStreamingReplyController {
    state: FeishuStreamingCardState,
    reply_mode: Option<FeishuStreamingReplyMode>,
    last_card_update_at_ms: u64,
}

impl FeishuStreamingReplyController {
    fn new(user_message: String, started_at_ms: u64) -> Self {
        Self {
            state: FeishuStreamingCardState::new(user_message, started_at_ms),
            reply_mode: None,
            last_card_update_at_ms: started_at_ms,
        }
    }

    fn update_from_snapshot(&mut self, snapshot: &CurrentTurnSnapshot) {
        update_feishu_card_state_from_snapshot(&mut self.state, snapshot);
    }

    fn update_status(&mut self, status: crate::sse::TurnStatus) {
        self.state.status = status;
    }

    fn apply_stream(&mut self, event: &agent_core::StreamEvent) {
        apply_stream_event_to_feishu_card_state(&mut self.state, event);
    }

    async fn flush(
        &mut self,
        profile: &ChannelProfile,
        force: bool,
    ) -> Option<FeishuStreamingReplyMode> {
        self.reply_mode = maybe_update_reply(
            profile,
            self.reply_mode.take(),
            &self.state,
            &mut self.last_card_update_at_ms,
            force,
        )
        .await;
        self.reply_mode.clone()
    }
}

async fn create_feishu_streaming_reply_mode(
    profile: &ChannelProfile,
    reply_target: &FeishuMessageTarget,
    card_state: &FeishuStreamingCardState,
    request_uuid_seed: &str,
) -> Option<FeishuStreamingReplyMode> {
    match create_cardkit_entity(profile, &build_feishu_cardkit_shell()).await {
        Ok(card_id) => {
            match send_cardkit_message(profile, reply_target, &card_id, request_uuid_seed).await {
                Ok(message_id) => {
                    Some(FeishuStreamingReplyMode::CardKit { card_id, message_id, sequence: 1 })
                }
                Err(error) => {
                    eprintln!(
                        "发送飞书 CardKit 引用消息失败，退回 interactive patch profile_id={} message_id={} error={error}",
                        profile.id, request_uuid_seed
                    );
                    create_interactive_patch_mode(
                        profile,
                        reply_target,
                        card_state,
                        request_uuid_seed,
                    )
                    .await
                }
            }
        }
        Err(error) => {
            eprintln!(
                "创建飞书 CardKit 实体失败，退回 interactive patch profile_id={} message_id={} error={error}",
                profile.id, request_uuid_seed
            );
            create_interactive_patch_mode(profile, reply_target, card_state, request_uuid_seed)
                .await
        }
    }
}

async fn create_interactive_patch_mode(
    profile: &ChannelProfile,
    reply_target: &FeishuMessageTarget,
    card_state: &FeishuStreamingCardState,
    request_uuid_seed: &str,
) -> Option<FeishuStreamingReplyMode> {
    send_card_message(
        profile,
        reply_target,
        &build_feishu_card_payload(card_state),
        request_uuid_seed,
    )
    .await
    .map(|message_id| FeishuStreamingReplyMode::InteractivePatch { message_id })
    .map(Some)
    .unwrap_or_else(|error| {
        eprintln!(
            "创建飞书流式卡片失败，退回最终文本回复 profile_id={} message_id={} error={error}",
            profile.id, request_uuid_seed
        );
        None
    })
}

async fn prepare_session_for_turn(
    session_manager: &SessionManagerHandle,
    session_id: &str,
) -> Result<(), String> {
    let stats = session_manager
        .get_session_info(session_id.to_string())
        .await
        .map_err(|error| error.message)?;
    if stats.pressure_ratio.is_some_and(|ratio| ratio >= agent_prompts::AUTO_COMPRESSION_THRESHOLD)
    {
        session_manager
            .auto_compress_session(session_id.to_string())
            .await
            .map_err(|error| error.message)?;
    }
    Ok(())
}

async fn resolve_session_id(
    profile: &ChannelProfile,
    deps: &FeishuRuntimeDeps,
    key: &ExternalConversationKey,
    event: &EventBody,
) -> Result<String, String> {
    if let Some(binding) = deps
        .store
        .get_channel_binding_async(key.clone())
        .await
        .map_err(|error| error.to_string())?
    {
        return Ok(binding.session_id);
    }

    let title = build_session_title(profile, event);
    let session =
        deps.session_manager.create_session(Some(title)).await.map_err(|error| error.message)?;
    deps.store
        .upsert_channel_binding_async(ChannelSessionBinding::new(key.clone(), session.id.clone()))
        .await
        .map_err(|error| error.to_string())?;
    Ok(session.id)
}

fn runtime_error_to_string(error: RuntimeWorkerError) -> String {
    error.message
}

fn extract_final_answer_from_turn(turn: &agent_runtime::TurnLifecycle) -> Option<String> {
    let assistant_blocks = extract_assistant_segments_from_turn(turn);

    if !assistant_blocks.is_empty() {
        return Some(assistant_blocks.join("\n\n"));
    }

    turn.assistant_message.clone().filter(|message| !message.trim().is_empty())
}

fn build_turn_prompt(prompt: &str, event: &EventBody) -> String {
    if event.message.chat_type == "group" {
        format!(
            "来自飞书群聊的消息\n发送者 open_id: {}\n消息内容:\n{}",
            event.sender.sender_id.open_id, prompt
        )
    } else {
        prompt.to_string()
    }
}

fn build_session_title(profile: &ChannelProfile, event: &EventBody) -> String {
    match event.message.chat_type.as_str() {
        "p2p" => format!("Feishu DM · {} · {}", profile.name, event.sender.sender_id.open_id),
        _ if profile.config.thread_mode
            && !event.message.thread_id.as_deref().unwrap_or_default().is_empty() =>
        {
            format!(
                "Feishu Thread · {} · {}",
                profile.name,
                event.message.thread_id.as_deref().unwrap_or_default()
            )
        }
        _ => format!(
            "Feishu Group · {} · {}",
            profile.name,
            event.message.chat_id.clone().unwrap_or_default()
        ),
    }
}

async fn send_reply_message(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    text: &str,
    request_uuid_seed: &str,
) -> Result<(), String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let request = prepare_send_message_request(profile, target, text, request_uuid_seed);
    let response = client
        .post(request.url)
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&request.body)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "发送飞书回复失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    let payload: FeishuSimpleResponse = response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("发送飞书回复失败: {}", payload.msg));
    }
    Ok(())
}

async fn send_card_message(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    card: &serde_json::Value,
    request_uuid_seed: &str,
) -> Result<String, String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let request = prepare_send_card_request(profile, target, card, request_uuid_seed);
    let response = client
        .post(request.url)
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&request.body)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "发送飞书卡片失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuMessageResponse =
        response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("发送飞书卡片失败: {}", payload.msg));
    }
    payload
        .data
        .and_then(|data| data.message_id)
        .filter(|message_id| !message_id.is_empty())
        .ok_or_else(|| "发送飞书卡片失败: 缺少 message_id".to_string())
}

async fn send_cardkit_message(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    card_id: &str,
    request_uuid_seed: &str,
) -> Result<String, String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let request = prepare_send_cardkit_request(profile, target, card_id, request_uuid_seed);
    let response = client
        .post(request.url)
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&request.body)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "发送飞书 CardKit 引用消息失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuMessageResponse =
        response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("发送飞书 CardKit 引用消息失败: {}", payload.msg));
    }
    payload
        .data
        .and_then(|data| data.message_id)
        .filter(|message_id| !message_id.is_empty())
        .ok_or_else(|| "发送飞书 CardKit 引用消息失败: 缺少 message_id".to_string())
}

async fn create_cardkit_entity(
    profile: &ChannelProfile,
    card: &serde_json::Value,
) -> Result<String, String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .post(format!("{base_url}{FEISHU_CARDKIT_CARDS_URI}"))
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "type": "card_json",
            "data": serde_json::to_string(card).map_err(|error| error.to_string())?,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "创建飞书 CardKit 实体失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuCardKitCreateResponse =
        response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("创建飞书 CardKit 实体失败: {}", payload.msg));
    }
    payload
        .data
        .and_then(|data| data.card_id)
        .filter(|card_id: &String| !card_id.is_empty())
        .ok_or_else(|| "创建飞书 CardKit 实体失败: 缺少 card_id".to_string())
}

async fn stream_cardkit_content(
    profile: &ChannelProfile,
    card_id: &str,
    content: &str,
    sequence: u64,
) -> Result<(), String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .put(format!(
            "{base_url}/{}",
            FEISHU_CARDKIT_CARD_ELEMENT_CONTENT_URI
                .replace("{card_id}", card_id)
                .replace("{element_id}", FEISHU_CARDKIT_STREAMING_ELEMENT_ID)
                .trim_start_matches('/')
        ))
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&build_cardkit_stream_payload(content, sequence))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "流式更新飞书 CardKit 内容失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuSimpleResponse = response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("流式更新飞书 CardKit 内容失败: {}", payload.msg));
    }
    Ok(())
}

async fn set_cardkit_streaming_mode(
    profile: &ChannelProfile,
    card_id: &str,
    streaming_mode: bool,
    sequence: u64,
) -> Result<(), String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .patch(format!(
            "{base_url}/{}",
            FEISHU_CARDKIT_CARD_SETTINGS_URI.replace("{card_id}", card_id).trim_start_matches('/')
        ))
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "settings": serde_json::to_string(&json!({ "streaming_mode": streaming_mode }))
                .map_err(|error| error.to_string())?,
            "sequence": sequence,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "切换飞书 CardKit streaming_mode 失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuSimpleResponse = response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("切换飞书 CardKit streaming_mode 失败: {}", payload.msg));
    }
    Ok(())
}

async fn update_cardkit_card(
    profile: &ChannelProfile,
    card_id: &str,
    card: &serde_json::Value,
    sequence: u64,
) -> Result<(), String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .put(format!(
            "{base_url}/{}",
            FEISHU_CARDKIT_CARD_UPDATE_URI.replace("{card_id}", card_id).trim_start_matches('/')
        ))
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "card": {
                "type": "card_json",
                "data": serde_json::to_string(card).map_err(|error| error.to_string())?,
            },
            "sequence": sequence,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "更新飞书 CardKit 卡片失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuSimpleResponse = response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("更新飞书 CardKit 卡片失败: {}", payload.msg));
    }
    Ok(())
}

async fn update_card_message(
    profile: &ChannelProfile,
    message_id: &str,
    card: &serde_json::Value,
) -> Result<(), String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .patch(format!("{base_url}{FEISHU_MESSAGES_URI}/{message_id}"))
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "content": serde_json::to_string(card).map_err(|error| error.to_string())?,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "更新飞书卡片失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuSimpleResponse = response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("更新飞书卡片失败: {}", payload.msg));
    }
    Ok(())
}

async fn maybe_update_reply(
    profile: &ChannelProfile,
    reply_mode: Option<FeishuStreamingReplyMode>,
    state: &FeishuStreamingCardState,
    last_card_update_at_ms: &mut u64,
    force: bool,
) -> Option<FeishuStreamingReplyMode> {
    let Some(reply_mode) = reply_mode else {
        return None;
    };
    let now_ms = current_timestamp_ms();
    if !force && now_ms.saturating_sub(*last_card_update_at_ms) < FEISHU_CARD_UPDATE_INTERVAL_MS {
        return Some(reply_mode);
    }

    match reply_mode {
        FeishuStreamingReplyMode::CardKit { card_id, message_id, mut sequence } => {
            let result = if force {
                sequence += 1;
                if let Err(error) =
                    set_cardkit_streaming_mode(profile, &card_id, false, sequence).await
                {
                    Err(error)
                } else {
                    sequence += 1;
                    update_cardkit_card(
                        profile,
                        &card_id,
                        &build_feishu_card_payload(state),
                        sequence,
                    )
                    .await
                }
            } else {
                sequence += 1;
                stream_cardkit_content(
                    profile,
                    &card_id,
                    &build_feishu_cardkit_stream_markdown(state),
                    sequence,
                )
                .await
            };

            match result {
                Ok(()) => {
                    *last_card_update_at_ms = now_ms;
                    Some(FeishuStreamingReplyMode::CardKit { card_id, message_id, sequence })
                }
                Err(error) => {
                    if force {
                        eprintln!(
                            "更新飞书最终 CardKit 卡片失败，退回最终文本回复 profile_id={} card_id={} error={error}",
                            profile.id, card_id
                        );
                        None
                    } else {
                        eprintln!(
                            "流式更新飞书 CardKit 失败，切回 interactive patch profile_id={} card_id={} error={error}",
                            profile.id, card_id
                        );
                        Some(FeishuStreamingReplyMode::InteractivePatch { message_id })
                    }
                }
            }
        }
        FeishuStreamingReplyMode::InteractivePatch { message_id } => {
            let card = build_feishu_card_payload(state);
            match update_card_message(profile, &message_id, &card).await {
                Ok(()) => {
                    *last_card_update_at_ms = now_ms;
                    Some(FeishuStreamingReplyMode::InteractivePatch { message_id })
                }
                Err(error) => {
                    if force {
                        eprintln!(
                            "更新飞书最终卡片失败，退回最终文本回复 profile_id={} message_id={} error={error}",
                            profile.id, message_id
                        );
                        None
                    } else {
                        eprintln!(
                            "更新飞书流式卡片失败，退回最终文本回复 profile_id={} message_id={} error={error}",
                            profile.id, message_id
                        );
                        None
                    }
                }
            }
        }
    }
}

async fn add_processing_reaction(
    profile: &ChannelProfile,
    message_id: &str,
) -> Result<String, String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let url = feishu_message_reactions_url(profile, message_id);
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "reaction_type": {
                "emoji_type": FEISHU_PROCESSING_EMOJI_TYPE,
            }
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "添加飞书处理中表情失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuReactionCreateResponse =
        response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("添加飞书处理中表情失败: {}", payload.msg));
    }
    payload
        .data
        .and_then(|data| data.reaction_id)
        .ok_or_else(|| "添加飞书处理中表情失败: 缺少 reaction_id".to_string())
}

async fn remove_message_reaction(
    profile: &ChannelProfile,
    message_id: &str,
    reaction_id: &str,
) -> Result<(), String> {
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("{}/{}", feishu_message_reactions_url(profile, message_id), reaction_id);
    let response = client
        .delete(url)
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "移除飞书处理中表情失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuSimpleResponse = response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("移除飞书处理中表情失败: {}", payload.msg));
    }
    Ok(())
}

fn feishu_message_reactions_url(profile: &ChannelProfile, message_id: &str) -> String {
    let base_url = profile.config.base_url.trim_end_matches('/');
    format!(
        "{base_url}/{}",
        FEISHU_MESSAGE_REACTIONS_URI.replace("{message_id}", message_id).trim_start_matches('/')
    )
}

async fn resolve_p2p_chat_id(profile: &ChannelProfile, open_id: &str) -> Result<String, String> {
    if open_id.trim().is_empty() {
        return Err("p2p 消息缺少 sender.open_id，无法解析 chat_id".into());
    }
    let tenant_access_token = fetch_tenant_access_token(profile).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .post(format!(
            "{base_url}/{}?user_id_type=open_id",
            FEISHU_P2P_CHAT_QUERY_URI.trim_start_matches('/')
        ))
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "chatter_ids": [open_id],
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "解析飞书单聊 chat_id 失败: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let payload: FeishuP2pChatQueryResponse =
        response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("解析飞书单聊 chat_id 失败: {}", payload.msg));
    }
    payload
        .data
        .and_then(|data| data.p2p_chats)
        .and_then(|mut chats| chats.drain(..).next())
        .map(|chat| chat.chat_id)
        .filter(|chat_id| !chat_id.is_empty())
        .ok_or_else(|| format!("未找到 open_id={} 对应的飞书单聊 chat_id", open_id))
}

async fn fetch_tenant_access_token(profile: &ChannelProfile) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = profile.config.base_url.trim_end_matches('/');
    let response = client
        .post(format!("{base_url}/open-apis/auth/v3/tenant_access_token/internal"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "app_id": profile.config.app_id,
            "app_secret": profile.config.app_secret,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let payload: TenantAccessTokenResponse =
        response.json().await.map_err(|error| error.to_string())?;
    if payload.code != 0 {
        return Err(format!("获取 tenant_access_token 失败: {}", payload.msg));
    }
    Ok(payload.tenant_access_token)
}

fn extract_text(content: &str) -> Result<String, String> {
    let content_json: TextContent =
        serde_json::from_str(content).map_err(|error| error.to_string())?;
    Ok(content_json.text)
}

fn normalize_text(text: &str, mentions: &[Mention]) -> String {
    let mut normalized = text.to_string();
    for mention in mentions {
        normalized = normalized.replace(&mention.key, "");
    }
    normalized.trim().to_string()
}

fn resolve_conversation_key(
    profile: &ChannelProfile,
    event: &EventBody,
) -> Result<ExternalConversationKey, String> {
    if event.message.chat_type == "p2p" {
        if event.sender.sender_id.open_id.is_empty() {
            return Err("p2p 消息缺少 sender.open_id".into());
        }
        return Ok(ExternalConversationKey {
            channel_kind: "feishu".into(),
            profile_id: profile.id.clone(),
            scope: "p2p".into(),
            conversation_key: event.sender.sender_id.open_id.clone(),
        });
    }

    if profile.config.thread_mode
        && let Some(thread_id) = &event.message.thread_id
        && !thread_id.is_empty()
    {
        return Ok(ExternalConversationKey {
            channel_kind: "feishu".into(),
            profile_id: profile.id.clone(),
            scope: "thread".into(),
            conversation_key: thread_id.clone(),
        });
    }

    let chat_id = event
        .message
        .chat_id
        .clone()
        .or_else(|| event.chat.as_ref().map(|chat| chat.chat_id.clone()))
        .filter(|chat_id| !chat_id.is_empty())
        .ok_or_else(|| "群聊消息缺少 chat_id".to_string())?;

    Ok(ExternalConversationKey {
        channel_kind: "feishu".into(),
        profile_id: profile.id.clone(),
        scope: "group".into(),
        conversation_key: chat_id,
    })
}

async fn resolve_reply_target(
    profile: &ChannelProfile,
    event: &EventBody,
) -> Result<FeishuMessageTarget, String> {
    let is_p2p = event.message.chat_type == "p2p";
    let receive_id = if is_p2p {
        if let Some(chat_id) = resolve_p2p_chat_id_from_event(event) {
            chat_id.to_string()
        } else {
            resolve_p2p_chat_id(profile, &event.sender.sender_id.open_id).await?
        }
    } else {
        event
            .message
            .chat_id
            .clone()
            .or_else(|| event.chat.as_ref().map(|chat| chat.chat_id.clone()))
            .filter(|chat_id| !chat_id.is_empty())
            .ok_or_else(|| "群聊消息缺少 chat_id".to_string())?
    };

    Ok(FeishuMessageTarget {
        receive_id,
        receive_id_type: "chat_id".into(),
        reply_to_message_id: (!is_p2p).then_some(event.message.message_id.clone()),
        reply_in_thread: !is_p2p && profile.config.thread_mode,
    })
}

fn resolve_p2p_chat_id_from_event(event: &EventBody) -> Option<&str> {
    event.message.chat_id.as_deref().filter(|chat_id| !chat_id.is_empty()).or_else(|| {
        event.chat.as_ref().map(|chat| chat.chat_id.as_str()).filter(|chat_id| !chat_id.is_empty())
    })
}

struct PreparedFeishuSendRequest {
    url: String,
    body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FeishuStreamingReplyMode {
    CardKit { card_id: String, message_id: String, sequence: u64 },
    InteractivePatch { message_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FeishuStreamingToolStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeishuStreamingToolState {
    invocation_id: String,
    tool_name: String,
    status: FeishuStreamingToolStatus,
    output: String,
    failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeishuStreamingCardState {
    user_message: String,
    status: crate::sse::TurnStatus,
    reasoning: String,
    streaming_text: String,
    completed_segments: Vec<String>,
    tools: Vec<FeishuStreamingToolState>,
    started_at_ms: u64,
    finished_at_ms: Option<u64>,
    failed: bool,
}

impl FeishuStreamingCardState {
    fn new(user_message: String, started_at_ms: u64) -> Self {
        Self {
            user_message,
            status: crate::sse::TurnStatus::Waiting,
            reasoning: String::new(),
            streaming_text: String::new(),
            completed_segments: Vec::new(),
            tools: Vec::new(),
            started_at_ms,
            finished_at_ms: None,
            failed: false,
        }
    }

    fn final_text(&self) -> String {
        if !self.completed_segments.is_empty() {
            self.completed_segments.join("\n\n")
        } else if !self.streaming_text.trim().is_empty() {
            self.streaming_text.clone()
        } else if self.failed {
            "处理失败，且没有可发送的回复内容。".into()
        } else {
            "已完成处理，但没有生成可发送的文本回复。".into()
        }
    }

    fn summary_text(&self) -> String {
        if !self.completed_segments.is_empty() {
            self.final_text().chars().take(120).collect::<String>()
        } else if !self.streaming_text.trim().is_empty() {
            self.streaming_text.chars().take(120).collect::<String>()
        } else if !self.reasoning.trim().is_empty() {
            format!("{}…", self.reasoning.chars().take(60).collect::<String>())
        } else if self.failed {
            "飞书会话处理失败".into()
        } else {
            "飞书会话处理中…".into()
        }
    }
}

fn build_feishu_card_payload(state: &FeishuStreamingCardState) -> serde_json::Value {
    let mut elements = Vec::<serde_json::Value>::new();

    if !state.reasoning.trim().is_empty() {
        elements.push(json!({
            "tag": "collapsible_panel",
            "expanded": false,
            "header": {
                "title": {
                    "tag": "markdown",
                    "content": "💭 思考过程",
                },
                "vertical_align": "center",
                "icon": {
                    "tag": "standard_icon",
                    "token": "down-small-ccm_outlined",
                    "size": "16px 16px"
                },
                "icon_position": "follow_text",
                "icon_expanded_angle": -180
            },
            "border": { "color": "grey", "corner_radius": "5px" },
            "vertical_spacing": "8px",
            "padding": "8px 8px 8px 8px",
            "elements": [{
                "tag": "markdown",
                "content": feishu_markdown_text(&state.reasoning),
                "text_size": "notation",
            }]
        }));
    }

    let main_content = if !state.completed_segments.is_empty() {
        feishu_markdown_text(&state.final_text())
    } else if !state.streaming_text.trim().is_empty() {
        feishu_markdown_text(&state.streaming_text)
    } else if !state.reasoning.trim().is_empty() {
        "_正在生成回复…_".into()
    } else {
        "_正在思考…_".into()
    };
    elements.push(json!({
        "tag": "markdown",
        "content": main_content,
    }));

    if !state.tools.is_empty() {
        let tool_lines = state
            .tools
            .iter()
            .map(|tool| {
                let icon = match tool.status {
                    FeishuStreamingToolStatus::Running => "🔄",
                    FeishuStreamingToolStatus::Completed => "✅",
                    FeishuStreamingToolStatus::Failed => "❌",
                };
                format!("{icon} **{}**", tool.tool_name)
            })
            .collect::<Vec<_>>()
            .join("\n");
        elements.push(json!({
            "tag": "markdown",
            "content": tool_lines,
            "text_size": "notation",
        }));
    }

    let elapsed_ms = state
        .finished_at_ms
        .unwrap_or_else(current_timestamp_ms)
        .saturating_sub(state.started_at_ms);
    elements.push(json!({
        "tag": "markdown",
        "content": format!("{} · 耗时 {}", feishu_status_footer(state), format_elapsed_ms(elapsed_ms)),
        "text_size": "notation",
    }));

    json!({
        "schema": "2.0",
        "config": {
            "wide_screen_mode": true,
            "update_multi": true,
            "summary": {
                "content": state.summary_text()
            }
        },
        "body": {
            "elements": elements,
        }
    })
}

fn build_feishu_cardkit_shell() -> serde_json::Value {
    json!({
        "schema": "2.0",
        "config": {
            "streaming_mode": true,
            "summary": {
                "content": "飞书会话处理中…"
            }
        },
        "body": {
            "elements": [{
                "tag": "markdown",
                "content": "",
                "element_id": FEISHU_CARDKIT_STREAMING_ELEMENT_ID,
                "text_align": "left",
                "text_size": "normal_v2",
                "margin": "0px 0px 0px 0px"
            }]
        }
    })
}

fn build_feishu_cardkit_stream_markdown(state: &FeishuStreamingCardState) -> String {
    let mut sections = Vec::<String>::new();

    if !state.completed_segments.is_empty() {
        sections.push(feishu_markdown_text(&state.final_text()));
    } else if !state.streaming_text.trim().is_empty() {
        sections.push(feishu_markdown_text(&state.streaming_text));
    } else if !state.reasoning.trim().is_empty() {
        sections.push(format!("💭 {}", feishu_markdown_text(&state.reasoning)));
    } else {
        sections.push("_正在思考…_".into());
    }

    if !state.tools.is_empty() {
        let tool_lines = state
            .tools
            .iter()
            .map(|tool| {
                let icon = match tool.status {
                    FeishuStreamingToolStatus::Running => "🔄",
                    FeishuStreamingToolStatus::Completed => "✅",
                    FeishuStreamingToolStatus::Failed => "❌",
                };
                format!("{icon} **{}**", tool.tool_name)
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(tool_lines);
    }

    sections.join("\n\n")
}

fn build_cardkit_stream_payload(content: &str, sequence: u64) -> serde_json::Value {
    json!({
        "content": content,
        "sequence": sequence,
    })
}

fn apply_stream_event_to_feishu_card_state(
    state: &mut FeishuStreamingCardState,
    event: &agent_core::StreamEvent,
) {
    match event {
        agent_core::StreamEvent::ThinkingDelta { text } => {
            state.reasoning.push_str(text);
            state.status = crate::sse::TurnStatus::Thinking;
        }
        agent_core::StreamEvent::TextDelta { text } => {
            state.streaming_text.push_str(text);
            state.status = crate::sse::TurnStatus::Generating;
        }
        agent_core::StreamEvent::ToolCallDetected { invocation_id, tool_name, .. }
        | agent_core::StreamEvent::ToolCallStarted { invocation_id, tool_name, .. } => {
            let tool = find_or_insert_tool_state(state, invocation_id, tool_name);
            tool.status = FeishuStreamingToolStatus::Running;
            state.status = crate::sse::TurnStatus::Working;
        }
        agent_core::StreamEvent::ToolOutputDelta { invocation_id, text, .. } => {
            if let Some(tool) =
                state.tools.iter_mut().find(|tool| tool.invocation_id == *invocation_id)
            {
                tool.output.push_str(text);
            }
            state.status = crate::sse::TurnStatus::Working;
        }
        agent_core::StreamEvent::ToolCallCompleted {
            invocation_id,
            tool_name,
            content,
            failed,
            ..
        } => {
            let tool = find_or_insert_tool_state(state, invocation_id, tool_name);
            tool.status = if *failed {
                FeishuStreamingToolStatus::Failed
            } else {
                FeishuStreamingToolStatus::Completed
            };
            tool.failed = *failed;
            if !content.is_empty() {
                if !tool.output.is_empty() {
                    tool.output.push('\n');
                }
                tool.output.push_str(content);
            }
            state.status = crate::sse::TurnStatus::Working;
        }
        agent_core::StreamEvent::Done => {
            state.finished_at_ms = Some(current_timestamp_ms());
        }
        agent_core::StreamEvent::Log { .. } => {}
    }
}

fn update_feishu_card_state_from_snapshot(
    state: &mut FeishuStreamingCardState,
    snapshot: &CurrentTurnSnapshot,
) {
    state.user_message = snapshot.user_message.clone();
    state.status = snapshot.status.clone();
}

fn finalize_feishu_card_state(
    state: &mut FeishuStreamingCardState,
    completed_segments: Option<Vec<String>>,
    failure_message: Option<String>,
    finished_at_ms: u64,
) {
    if let Some(segments) = completed_segments.filter(|segments| !segments.is_empty()) {
        state.completed_segments = segments;
        state.streaming_text.clear();
    }
    if let Some(message) = failure_message {
        state.failed = true;
        if state.completed_segments.is_empty() && state.streaming_text.trim().is_empty() {
            state.streaming_text = message;
        }
    }
    state.finished_at_ms = Some(finished_at_ms);
    state.status = if state.failed {
        crate::sse::TurnStatus::Cancelled
    } else {
        crate::sse::TurnStatus::Generating
    };
}

fn extract_assistant_segments_from_turn(turn: &agent_runtime::TurnLifecycle) -> Vec<String> {
    turn.blocks
        .iter()
        .filter_map(|block| match block {
            agent_runtime::TurnBlock::Assistant { content } if !content.trim().is_empty() => {
                Some(content.clone())
            }
            _ => None,
        })
        .collect()
}

fn find_or_insert_tool_state<'a>(
    state: &'a mut FeishuStreamingCardState,
    invocation_id: &str,
    tool_name: &str,
) -> &'a mut FeishuStreamingToolState {
    if let Some(index) = state.tools.iter().position(|tool| tool.invocation_id == invocation_id) {
        return &mut state.tools[index];
    }
    state.tools.push(FeishuStreamingToolState {
        invocation_id: invocation_id.to_string(),
        tool_name: tool_name.to_string(),
        status: FeishuStreamingToolStatus::Running,
        output: String::new(),
        failed: false,
    });
    let last_index = state.tools.len().saturating_sub(1);
    &mut state.tools[last_index]
}

fn feishu_status_footer(state: &FeishuStreamingCardState) -> &'static str {
    if state.failed {
        "失败"
    } else if state.finished_at_ms.is_some() {
        "完成"
    } else {
        "处理中"
    }
}

fn feishu_markdown_text(text: &str) -> String {
    text.trim().to_string()
}

fn format_elapsed_ms(elapsed_ms: u64) -> String {
    if elapsed_ms >= 1000 {
        format!("{:.1} 秒", elapsed_ms as f64 / 1000.0)
    } else {
        format!("{} 毫秒", elapsed_ms)
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn prepare_send_message_request(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    text: &str,
    request_uuid_seed: &str,
) -> PreparedFeishuSendRequest {
    let base_url = profile.config.base_url.trim_end_matches('/');
    let content = json!({ "text": text }).to_string();
    let uuid = format!("aia-{request_uuid_seed}");
    if let Some(reply_to_message_id) = &target.reply_to_message_id {
        return PreparedFeishuSendRequest {
            url: format!("{base_url}/open-apis/im/v1/messages/{reply_to_message_id}/reply"),
            body: json!({
                "msg_type": "text",
                "content": content,
                "reply_in_thread": target.reply_in_thread,
                "uuid": uuid,
            }),
        };
    }

    PreparedFeishuSendRequest {
        url: format!(
            "{base_url}/open-apis/im/v1/messages?receive_id_type={}",
            target.receive_id_type
        ),
        body: json!({
            "receive_id": target.receive_id,
            "msg_type": "text",
            "content": content,
            "uuid": uuid,
        }),
    }
}

fn prepare_send_card_request(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    card: &serde_json::Value,
    request_uuid_seed: &str,
) -> PreparedFeishuSendRequest {
    let base_url = profile.config.base_url.trim_end_matches('/');
    let content = serde_json::to_string(card).unwrap_or_else(|_| "{}".into());
    let uuid = format!("aia-card-{request_uuid_seed}");
    if let Some(reply_to_message_id) = &target.reply_to_message_id {
        return PreparedFeishuSendRequest {
            url: format!("{base_url}{FEISHU_MESSAGES_URI}/{reply_to_message_id}/reply"),
            body: json!({
                "msg_type": "interactive",
                "content": content,
                "reply_in_thread": target.reply_in_thread,
                "uuid": uuid,
            }),
        };
    }

    PreparedFeishuSendRequest {
        url: format!("{base_url}{FEISHU_MESSAGES_URI}?receive_id_type={}", target.receive_id_type),
        body: json!({
            "receive_id": target.receive_id,
            "msg_type": "interactive",
            "content": content,
            "uuid": uuid,
        }),
    }
}

fn prepare_send_cardkit_request(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    card_id: &str,
    request_uuid_seed: &str,
) -> PreparedFeishuSendRequest {
    let base_url = profile.config.base_url.trim_end_matches('/');
    let content = serde_json::to_string(&json!({
        "type": "card",
        "data": { "card_id": card_id }
    }))
    .unwrap_or_else(|_| "{}".into());
    let uuid = format!("aia-cardkit-{request_uuid_seed}");
    if let Some(reply_to_message_id) = &target.reply_to_message_id {
        return PreparedFeishuSendRequest {
            url: format!("{base_url}{FEISHU_MESSAGES_URI}/{reply_to_message_id}/reply"),
            body: json!({
                "msg_type": "interactive",
                "content": content,
                "reply_in_thread": target.reply_in_thread,
                "uuid": uuid,
            }),
        };
    }

    PreparedFeishuSendRequest {
        url: format!("{base_url}{FEISHU_MESSAGES_URI}?receive_id_type={}", target.receive_id_type),
        body: json!({
            "receive_id": target.receive_id,
            "msg_type": "interactive",
            "content": content,
            "uuid": uuid,
        }),
    }
}

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    header: EventHeader,
    event: EventBody,
}

#[derive(Debug, Deserialize)]
struct EventHeader {
    event_type: String,
}

#[derive(Debug, Deserialize)]
struct EventBody {
    sender: Sender,
    message: Message,
    #[serde(default)]
    chat: Option<Chat>,
}

#[derive(Debug, Deserialize)]
struct Sender {
    sender_id: SenderId,
    sender_type: String,
}

#[derive(Debug, Deserialize)]
struct SenderId {
    open_id: String,
}

#[derive(Debug, Deserialize)]
struct Message {
    message_id: String,
    message_type: String,
    content: String,
    chat_type: String,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    mentions: Vec<Mention>,
}

#[derive(Debug, Deserialize)]
struct Chat {
    chat_id: String,
}

#[derive(Debug, Deserialize)]
struct Mention {
    key: String,
}

#[derive(Debug, Deserialize)]
struct TextContent {
    text: String,
}

#[derive(Debug, Deserialize)]
struct TenantAccessTokenResponse {
    code: i32,
    msg: String,
    tenant_access_token: String,
}

#[derive(Debug, Deserialize)]
struct FeishuSimpleResponse {
    code: i32,
    msg: String,
}

#[derive(Debug, Deserialize)]
struct FeishuMessageResponse {
    code: i32,
    msg: String,
    data: Option<FeishuMessageResponseData>,
}

#[derive(Debug, Deserialize)]
struct FeishuMessageResponseData {
    message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeishuCardKitCreateResponse {
    code: i32,
    msg: String,
    data: Option<FeishuCardKitCreateResponseData>,
}

#[derive(Debug, Deserialize)]
struct FeishuCardKitCreateResponseData {
    card_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeishuReactionCreateResponse {
    code: i32,
    msg: String,
    data: Option<FeishuReactionCreateData>,
}

#[derive(Debug, Deserialize)]
struct FeishuReactionCreateData {
    reaction_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeishuP2pChatQueryResponse {
    code: i32,
    msg: String,
    data: Option<FeishuP2pChatQueryData>,
}

#[derive(Debug, Deserialize)]
struct FeishuP2pChatQueryData {
    p2p_chats: Option<Vec<FeishuP2pChatItem>>,
}

#[derive(Debug, Deserialize)]
struct FeishuP2pChatItem {
    chat_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FeishuWebsocketEndpointResponse {
    code: i32,
    msg: String,
    data: Option<FeishuWebsocketEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeishuWebsocketEndpoint {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "ClientConfig")]
    client_config: Option<FeishuServerClientConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FeishuServerClientConfig {
    #[serde(rename = "ReconnectCount", default)]
    reconnect_count: i64,
    #[serde(rename = "ReconnectInterval", default)]
    reconnect_interval: u64,
    #[serde(rename = "ReconnectNonce", default)]
    reconnect_nonce: u64,
    #[serde(rename = "PingInterval", default)]
    ping_interval: u64,
}

#[derive(Debug, Clone)]
struct FeishuConnectionPolicy {
    reconnect_interval: Duration,
    ping_interval: Duration,
    reconnect_count: Option<usize>,
    reconnect_nonce: Duration,
}

impl Default for FeishuConnectionPolicy {
    fn default() -> Self {
        Self {
            reconnect_interval: Duration::from_secs(120),
            ping_interval: Duration::from_secs(120),
            reconnect_count: None,
            reconnect_nonce: Duration::from_secs(30),
        }
    }
}

impl FeishuConnectionPolicy {
    fn apply_server_config(&mut self, config: Option<&FeishuServerClientConfig>) {
        let Some(config) = config else {
            return;
        };
        if config.reconnect_interval > 0 {
            self.reconnect_interval = Duration::from_secs(config.reconnect_interval);
        }
        if config.ping_interval > 0 {
            self.ping_interval = Duration::from_secs(config.ping_interval);
        }
        self.reconnect_count =
            (config.reconnect_count >= 0).then_some(config.reconnect_count as usize);
        if config.reconnect_nonce > 0 {
            self.reconnect_nonce = Duration::from_secs(config.reconnect_nonce);
        }
    }

    fn should_stop_reconnecting(&self, attempts: usize) -> bool {
        self.reconnect_count.is_some_and(|max| attempts >= max)
    }

    fn next_reconnect_jitter(&self) -> Option<Duration> {
        let upper_bound_ms = self.reconnect_nonce.as_millis();
        if upper_bound_ms == 0 {
            return None;
        }
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        Some(Duration::from_millis((nanos % upper_bound_ms) as u64))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeishuHeader {
    key: String,
    value: String,
}

impl FeishuHeader {
    fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self { key: key.into(), value: value.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeishuFrame {
    seq_id: u64,
    log_id: u64,
    service: i32,
    method: i32,
    headers: Vec<FeishuHeader>,
    payload_encoding: String,
    payload_type: String,
    payload: Vec<u8>,
    log_id_new: String,
}

impl FeishuFrame {
    fn new_ping(service_id: i32) -> Self {
        Self {
            seq_id: 0,
            log_id: 0,
            service: service_id,
            method: FEISHU_FRAME_TYPE_CONTROL,
            headers: vec![FeishuHeader::new(FEISHU_HEADER_TYPE, FEISHU_MESSAGE_TYPE_PING)],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        }
    }

    fn with_response_payload(&self, headers: Vec<FeishuHeader>, payload: Vec<u8>) -> Self {
        Self {
            seq_id: self.seq_id,
            log_id: self.log_id,
            service: self.service,
            method: self.method,
            headers,
            payload_encoding: self.payload_encoding.clone(),
            payload_type: self.payload_type.clone(),
            payload,
            log_id_new: self.log_id_new.clone(),
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        encode_varint_field(&mut buffer, 1, self.seq_id);
        encode_varint_field(&mut buffer, 2, self.log_id);
        encode_varint_field(&mut buffer, 3, self.service as u64);
        encode_varint_field(&mut buffer, 4, self.method as u64);
        for header in &self.headers {
            let mut header_buffer = Vec::new();
            encode_length_delimited_field(&mut header_buffer, 1, header.key.as_bytes());
            encode_length_delimited_field(&mut header_buffer, 2, header.value.as_bytes());
            encode_length_delimited_field(&mut buffer, 5, &header_buffer);
        }
        encode_length_delimited_field(&mut buffer, 6, self.payload_encoding.as_bytes());
        encode_length_delimited_field(&mut buffer, 7, self.payload_type.as_bytes());
        if !self.payload.is_empty() {
            encode_length_delimited_field(&mut buffer, 8, &self.payload);
        }
        encode_length_delimited_field(&mut buffer, 9, self.log_id_new.as_bytes());
        buffer
    }

    fn decode(input: &[u8]) -> Result<Self, String> {
        let mut cursor = 0;
        let mut frame = Self {
            seq_id: 0,
            log_id: 0,
            service: 0,
            method: 0,
            headers: Vec::new(),
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        };

        while cursor < input.len() {
            let tag = decode_varint(input, &mut cursor)?;
            let field_number = tag >> 3;
            let wire_type = tag & 0x07;
            match (field_number, wire_type) {
                (1, 0) => frame.seq_id = decode_varint(input, &mut cursor)?,
                (2, 0) => frame.log_id = decode_varint(input, &mut cursor)?,
                (3, 0) => frame.service = decode_varint(input, &mut cursor)? as i32,
                (4, 0) => frame.method = decode_varint(input, &mut cursor)? as i32,
                (5, 2) => {
                    let bytes = decode_length_delimited(input, &mut cursor)?;
                    frame.headers.push(decode_header(bytes)?);
                }
                (6, 2) => {
                    frame.payload_encoding =
                        String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                            .map_err(|error| error.to_string())?;
                }
                (7, 2) => {
                    frame.payload_type =
                        String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                            .map_err(|error| error.to_string())?;
                }
                (8, 2) => frame.payload = decode_length_delimited(input, &mut cursor)?.to_vec(),
                (9, 2) => {
                    frame.log_id_new =
                        String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                            .map_err(|error| error.to_string())?;
                }
                (_, _) => skip_unknown_field(input, &mut cursor, wire_type)?,
            }
        }

        Ok(frame)
    }
}

struct FeishuHeaders<'a> {
    inner: &'a [FeishuHeader],
}

impl<'a> FeishuHeaders<'a> {
    fn new(inner: &'a [FeishuHeader]) -> Self {
        Self { inner }
    }

    fn get_string(&self, key: &str) -> Option<&str> {
        self.inner.iter().find_map(|header| (header.key == key).then_some(header.value.as_str()))
    }

    fn get_usize(&self, key: &str) -> Option<usize> {
        self.get_string(key)?.parse().ok()
    }
}

#[derive(Default)]
struct PendingFrameBuffer {
    pending: HashMap<String, PendingMessageFrame>,
}

impl PendingFrameBuffer {
    fn push(
        &mut self,
        message_id: String,
        sum: usize,
        seq: usize,
        payload: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let entry =
            self.pending.entry(message_id.clone()).or_insert_with(|| PendingMessageFrame::new(sum));
        if seq >= entry.parts.len() {
            return None;
        }
        entry.parts[seq] = Some(payload);
        if !entry.parts.iter().all(Option::is_some) {
            return None;
        }
        let completed = self.pending.remove(&message_id)?;
        Some(completed.parts.into_iter().flatten().flatten().collect())
    }
}

struct PendingMessageFrame {
    parts: Vec<Option<Vec<u8>>>,
}

impl PendingMessageFrame {
    fn new(sum: usize) -> Self {
        Self { parts: vec![None; sum] }
    }
}

#[derive(Debug, Serialize)]
struct FeishuWsResponse {
    code: i32,
    headers: Option<HashMap<String, String>>,
    data: Option<Vec<u8>>,
}

impl FeishuWsResponse {
    fn ok() -> Self {
        Self { code: 200, headers: None, data: None }
    }

    fn encode(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|error| error.to_string())
    }
}

fn profile_fingerprint(profile: &ChannelProfile) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        profile.id,
        profile.enabled,
        profile.config.app_id,
        profile.config.app_secret,
        profile.config.base_url,
        profile.config.require_mention,
        profile.config.thread_mode,
    )
}

fn reconcile_runtime_workers(
    existing: &HashMap<String, RuntimeWorkerState>,
    desired_profiles: &[ChannelProfile],
) -> (Vec<String>, Vec<ChannelProfile>) {
    let desired_fingerprints = desired_profiles
        .iter()
        .map(|profile| (profile.id.clone(), profile_fingerprint(profile)))
        .collect::<HashMap<_, _>>();

    let stop_ids = existing
        .iter()
        .filter_map(|(profile_id, state)| match desired_fingerprints.get(profile_id) {
            Some(desired) if desired == &state.fingerprint && !state.finished => None,
            _ => Some(profile_id.clone()),
        })
        .collect::<Vec<_>>();
    let start_profiles = desired_profiles
        .iter()
        .filter(|profile| {
            existing.get(&profile.id).is_none_or(|state| {
                state.finished || state.fingerprint != profile_fingerprint(profile)
            })
        })
        .cloned()
        .collect::<Vec<_>>();

    (stop_ids, start_profiles)
}

fn encode_varint_field(buffer: &mut Vec<u8>, field_number: u64, value: u64) {
    encode_varint(buffer, field_number << 3);
    encode_varint(buffer, value);
}

fn encode_length_delimited_field(buffer: &mut Vec<u8>, field_number: u64, value: &[u8]) {
    encode_varint(buffer, (field_number << 3) | 0x02);
    encode_varint(buffer, value.len() as u64);
    buffer.extend_from_slice(value);
}

fn encode_varint(buffer: &mut Vec<u8>, mut value: u64) {
    loop {
        if value < 0x80 {
            buffer.push(value as u8);
            return;
        }
        buffer.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
}

fn decode_varint(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut shift = 0;
    let mut value = 0_u64;
    loop {
        if *cursor >= input.len() {
            return Err("飞书帧解码遇到意外结尾".into());
        }
        let byte = input[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 64 {
            return Err("飞书帧 varint 溢出".into());
        }
    }
}

fn decode_length_delimited<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let len = decode_varint(input, cursor)? as usize;
    if *cursor + len > input.len() {
        return Err("飞书帧长度字段越界".into());
    }
    let bytes = &input[*cursor..*cursor + len];
    *cursor += len;
    Ok(bytes)
}

fn decode_header(input: &[u8]) -> Result<FeishuHeader, String> {
    let mut cursor = 0;
    let mut key = None;
    let mut value = None;
    while cursor < input.len() {
        let tag = decode_varint(input, &mut cursor)?;
        let field_number = tag >> 3;
        let wire_type = tag & 0x07;
        match (field_number, wire_type) {
            (1, 2) => {
                key = Some(
                    String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                        .map_err(|error| error.to_string())?,
                );
            }
            (2, 2) => {
                value = Some(
                    String::from_utf8(decode_length_delimited(input, &mut cursor)?.to_vec())
                        .map_err(|error| error.to_string())?,
                );
            }
            (_, _) => skip_unknown_field(input, &mut cursor, wire_type)?,
        }
    }
    Ok(FeishuHeader {
        key: key.ok_or_else(|| "飞书帧 header 缺少 key".to_string())?,
        value: value.ok_or_else(|| "飞书帧 header 缺少 value".to_string())?,
    })
}

fn skip_unknown_field(input: &[u8], cursor: &mut usize, wire_type: u64) -> Result<(), String> {
    match wire_type {
        0 => {
            let _ = decode_varint(input, cursor)?;
        }
        1 => {
            *cursor = cursor.checked_add(8).ok_or_else(|| "飞书帧游标溢出".to_string())?;
        }
        2 => {
            let len = decode_varint(input, cursor)? as usize;
            *cursor = cursor.checked_add(len).ok_or_else(|| "飞书帧游标溢出".to_string())?;
        }
        5 => {
            *cursor = cursor.checked_add(4).ok_or_else(|| "飞书帧游标溢出".to_string())?;
        }
        _ => return Err(format!("飞书帧不支持的 wire type: {wire_type}")),
    }
    if *cursor > input.len() {
        return Err("飞书帧跳过未知字段越界".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use channel_registry::ChannelProfile;

    use super::*;

    fn sample_profile() -> ChannelProfile {
        ChannelProfile::new_feishu("default", "默认飞书", "cli_app", "secret")
    }

    fn group_event() -> EventBody {
        EventBody {
            sender: Sender {
                sender_id: SenderId { open_id: "ou_user_1".into() },
                sender_type: "user".into(),
            },
            message: Message {
                message_id: "om_123".into(),
                message_type: "text".into(),
                content: r#"{"text":"@_user_1 hello"}"#.into(),
                chat_type: "group".into(),
                chat_id: Some("oc_group_1".into()),
                thread_id: Some("omt_thread_1".into()),
                mentions: vec![Mention { key: "@_user_1".into() }],
            },
            chat: None,
        }
    }

    #[test]
    fn normalize_text_strips_mention_keys() {
        let text = normalize_text("@_user_1 hello world", &[Mention { key: "@_user_1".into() }]);

        assert_eq!(text, "hello world");
    }

    #[test]
    fn resolve_conversation_key_prefers_thread_when_enabled() {
        let profile = sample_profile();
        let key = resolve_conversation_key(&profile, &group_event()).expect("thread key");

        assert_eq!(key.scope, "thread");
        assert_eq!(key.conversation_key, "omt_thread_1");
    }

    #[test]
    fn resolve_conversation_key_uses_sender_for_p2p() {
        let profile = sample_profile();
        let event = EventBody {
            sender: Sender {
                sender_id: SenderId { open_id: "ou_p2p_user".into() },
                sender_type: "user".into(),
            },
            message: Message {
                message_id: "om_456".into(),
                message_type: "text".into(),
                content: r#"{"text":"hello"}"#.into(),
                chat_type: "p2p".into(),
                chat_id: None,
                thread_id: None,
                mentions: vec![],
            },
            chat: None,
        };

        let key = resolve_conversation_key(&profile, &event).expect("p2p key");
        assert_eq!(key.scope, "p2p");
        assert_eq!(key.conversation_key, "ou_p2p_user");
    }

    #[test]
    fn extract_text_reads_text_content() {
        let text = extract_text(r#"{"text":"hello"}"#).expect("text content");
        assert_eq!(text, "hello");
    }

    #[test]
    fn build_turn_prompt_marks_group_messages() {
        let prompt = build_turn_prompt("hello", &group_event());
        assert!(prompt.contains("飞书群聊"));
        assert!(prompt.contains("hello"));
    }

    #[test]
    fn webhook_challenge_round_trip() {
        let endpoint = FeishuWebsocketEndpointResponse {
            code: 0,
            msg: "ok".into(),
            data: Some(FeishuWebsocketEndpoint {
                url: "wss://open.feishu.cn/ws?service_id=12".into(),
                client_config: Some(FeishuServerClientConfig {
                    reconnect_count: 1,
                    reconnect_interval: 10,
                    reconnect_nonce: 2,
                    ping_interval: 5,
                }),
            }),
        };
        let encoded = serde_json::to_vec(&endpoint).expect("serialize endpoint");
        let decoded: FeishuWebsocketEndpointResponse =
            serde_json::from_slice(&encoded).expect("deserialize endpoint");
        assert_eq!(decoded.code, 0);
        assert_eq!(
            decoded.data.expect("endpoint data").url,
            "wss://open.feishu.cn/ws?service_id=12"
        );
    }

    #[test]
    fn frame_round_trip_preserves_required_fields() {
        let frame = FeishuFrame {
            seq_id: 1,
            log_id: 2,
            service: 3,
            method: FEISHU_FRAME_TYPE_DATA,
            headers: vec![
                FeishuHeader::new(FEISHU_HEADER_TYPE, FEISHU_MESSAGE_TYPE_EVENT),
                FeishuHeader::new(FEISHU_HEADER_MESSAGE_ID, "message-1"),
            ],
            payload_encoding: "json".into(),
            payload_type: "event".into(),
            payload: br#"{"ok":true}"#.to_vec(),
            log_id_new: "trace-1".into(),
        };

        let decoded = FeishuFrame::decode(&frame.encode()).expect("decode frame");

        assert_eq!(decoded, frame);
    }

    #[test]
    fn reconcile_runtime_workers_restarts_changed_profiles() {
        let existing = HashMap::from([
            (
                "default".to_string(),
                RuntimeWorkerState { fingerprint: "same".to_string(), finished: false },
            ),
            (
                "legacy".to_string(),
                RuntimeWorkerState { fingerprint: "old".to_string(), finished: false },
            ),
        ]);
        let mut desired = vec![sample_profile()];
        desired[0].id = "default".into();
        desired[0].config.base_url = "https://different.feishu.cn".into();

        let (stop_ids, start_profiles) = reconcile_runtime_workers(&existing, &desired);

        assert!(stop_ids.contains(&"default".to_string()));
        assert!(stop_ids.contains(&"legacy".to_string()));
        assert_eq!(start_profiles.len(), 1);
        assert_eq!(start_profiles[0].id, "default");
    }

    #[test]
    fn reconcile_runtime_workers_restarts_finished_workers_even_if_fingerprint_matches() {
        let mut profile = sample_profile();
        profile.id = "default".into();
        let fingerprint = profile_fingerprint(&profile);
        let existing = HashMap::from([(
            "default".to_string(),
            RuntimeWorkerState { fingerprint, finished: true },
        )]);

        let (stop_ids, start_profiles) = reconcile_runtime_workers(&existing, &[profile.clone()]);

        assert_eq!(stop_ids, vec!["default".to_string()]);
        assert_eq!(start_profiles, vec![profile]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resolve_reply_target_uses_chat_id_from_event_chat_for_p2p() {
        let profile = sample_profile();
        let event = EventBody {
            sender: Sender {
                sender_id: SenderId { open_id: "ou_p2p_user".into() },
                sender_type: "user".into(),
            },
            message: Message {
                message_id: "om_456".into(),
                message_type: "text".into(),
                content: r#"{"text":"hello"}"#.into(),
                chat_type: "p2p".into(),
                chat_id: None,
                thread_id: None,
                mentions: vec![],
            },
            chat: Some(Chat { chat_id: "oc_p2p_chat_1".into() }),
        };

        let target = resolve_reply_target(&profile, &event).await.expect("p2p target");
        assert_eq!(target.receive_id, "oc_p2p_chat_1");
        assert_eq!(target.receive_id_type, "chat_id");
        assert_eq!(target.reply_to_message_id, None);
        assert!(!target.reply_in_thread);
    }

    #[test]
    fn prepare_send_message_request_uses_create_for_direct_messages() {
        let mut profile = sample_profile();
        profile.config.base_url = "https://open.feishu.cn".into();
        let target = FeishuMessageTarget {
            receive_id: "oc_p2p_chat_1".into(),
            receive_id_type: "chat_id".into(),
            reply_to_message_id: None,
            reply_in_thread: false,
        };

        let request = prepare_send_message_request(&profile, &target, "你好", "msg-1");

        assert_eq!(
            request.url,
            "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=chat_id"
        );
        assert_eq!(request.body["receive_id"], "oc_p2p_chat_1");
        assert_eq!(request.body["uuid"], "aia-msg-1");
    }

    #[test]
    fn resolve_p2p_chat_id_from_event_prefers_message_then_chat_payload() {
        let event = EventBody {
            sender: Sender {
                sender_id: SenderId { open_id: "ou_p2p_user".into() },
                sender_type: "user".into(),
            },
            message: Message {
                message_id: "om_789".into(),
                message_type: "text".into(),
                content: r#"{"text":"hello"}"#.into(),
                chat_type: "p2p".into(),
                chat_id: Some("oc_p2p_chat_message".into()),
                thread_id: None,
                mentions: vec![],
            },
            chat: Some(Chat { chat_id: "oc_p2p_chat_fallback".into() }),
        };

        assert_eq!(resolve_p2p_chat_id_from_event(&event), Some("oc_p2p_chat_message"));
    }

    #[test]
    fn feishu_message_reactions_url_builds_expected_path() {
        let mut profile = sample_profile();
        profile.config.base_url = "https://open.feishu.cn".into();

        assert_eq!(
            feishu_message_reactions_url(&profile, "om_message_1"),
            "https://open.feishu.cn/open-apis/im/v1/messages/om_message_1/reactions"
        );
    }

    #[test]
    fn streaming_card_payload_renders_reasoning_answer_and_tool_status() {
        let state = FeishuStreamingCardState {
            user_message: "用户问题".into(),
            status: crate::sse::TurnStatus::Generating,
            reasoning: "先分析上下文".into(),
            streaming_text: String::new(),
            completed_segments: vec!["最终回答内容".into()],
            tools: vec![FeishuStreamingToolState {
                invocation_id: "tool-1".into(),
                tool_name: "grep".into(),
                status: FeishuStreamingToolStatus::Completed,
                output: "匹配到结果".into(),
                failed: false,
            }],
            started_at_ms: 1,
            finished_at_ms: Some(1301),
            failed: false,
        };

        let card = build_feishu_card_payload(&state);

        assert_eq!(card["schema"], "2.0");
        assert_eq!(card["config"]["update_multi"], true);
        let elements = card["body"]["elements"].as_array().expect("card elements");
        assert!(elements.iter().all(|element| {
            element["content"] != "### 飞书会话处理完成"
                && element["content"] != "**用户消息**\n用户问题"
        }));
        assert!(elements.iter().any(|element| {
            element["tag"] == "collapsible_panel"
                && element["elements"][0]["content"] == "先分析上下文"
        }));
        assert!(elements.iter().any(|element| {
            element["tag"] == "markdown" && element["content"] == "最终回答内容"
        }));
        assert!(elements.iter().any(|element| {
            element["tag"] == "markdown"
                && element["content"].as_str().is_some_and(|content| content.contains("grep"))
        }));
    }

    #[test]
    fn cardkit_streaming_shell_uses_single_streaming_element() {
        let card = build_feishu_cardkit_shell();

        assert_eq!(card["schema"], "2.0");
        assert_eq!(card["config"]["streaming_mode"], true);
        let elements = card["body"]["elements"].as_array().expect("card elements");
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0]["tag"], "markdown");
        assert_eq!(elements[0]["element_id"], FEISHU_CARDKIT_STREAMING_ELEMENT_ID);
    }

    #[test]
    fn prepare_send_cardkit_request_uses_card_reference_payload() {
        let mut profile = sample_profile();
        profile.config.base_url = "https://open.feishu.cn".into();
        let target = FeishuMessageTarget {
            receive_id: "oc_p2p_chat_1".into(),
            receive_id_type: "chat_id".into(),
            reply_to_message_id: None,
            reply_in_thread: false,
        };

        let request = prepare_send_cardkit_request(&profile, &target, "card_123", "msg-1");

        assert_eq!(
            request.url,
            "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=chat_id"
        );
        assert_eq!(request.body["msg_type"], "interactive");
        assert_eq!(
            request.body["content"],
            serde_json::to_string(&json!({
                "type": "card",
                "data": { "card_id": "card_123" }
            }))
            .expect("serialize card ref")
        );
    }

    #[test]
    fn build_cardkit_stream_payload_preserves_full_text_and_sequence() {
        let payload = build_cardkit_stream_payload("完整累计文本", 3);

        assert_eq!(payload["content"], "完整累计文本");
        assert_eq!(payload["sequence"], 3);
    }

    #[test]
    fn extract_final_answer_from_turn_keeps_multiple_assistant_blocks() {
        let turn = agent_runtime::TurnLifecycle {
            turn_id: "turn-1".into(),
            started_at_ms: 1,
            finished_at_ms: 2,
            source_entry_ids: vec![],
            user_message: "用户问题".into(),
            blocks: vec![
                agent_runtime::TurnBlock::Assistant { content: "第一段回复".into() },
                agent_runtime::TurnBlock::Assistant { content: "第二段回复".into() },
            ],
            assistant_message: Some("第二段回复".into()),
            thinking: None,
            tool_invocations: vec![],
            usage: None,
            failure_message: None,
            outcome: agent_runtime::TurnOutcome::Succeeded,
        };

        assert_eq!(extract_final_answer_from_turn(&turn), Some("第一段回复\n\n第二段回复".into()));
    }

    #[test]
    fn streaming_card_state_applies_stream_events_incrementally() {
        let mut state = FeishuStreamingCardState::new("用户问题".into(), 10);

        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::ThinkingDelta { text: "分析中".into() },
        );
        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::ToolCallStarted {
                invocation_id: "tool-1".into(),
                tool_name: "grep".into(),
                arguments: json!({ "pattern": "feishu" }),
            },
        );
        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::ToolOutputDelta {
                invocation_id: "tool-1".into(),
                stream: agent_core::ToolOutputStream::Stdout,
                text: "匹配内容".into(),
            },
        );
        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::ToolCallCompleted {
                invocation_id: "tool-1".into(),
                tool_name: "grep".into(),
                content: "完成".into(),
                details: None,
                failed: false,
            },
        );
        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::TextDelta { text: "最终回答".into() },
        );

        assert_eq!(state.reasoning, "分析中");
        assert_eq!(state.streaming_text, "最终回答");
        assert_eq!(state.tools.len(), 1);
        assert_eq!(state.tools[0].tool_name, "grep");
        assert_eq!(state.tools[0].status, FeishuStreamingToolStatus::Completed);
        assert!(state.tools[0].output.contains("匹配内容"));
    }

    #[test]
    fn streaming_display_text_uses_inflight_text_before_final_blocks() {
        let mut state = FeishuStreamingCardState::new("用户问题".into(), 10);

        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::TextDelta { text: "第一段流式".into() },
        );
        apply_stream_event_to_feishu_card_state(
            &mut state,
            &agent_core::StreamEvent::TextDelta { text: "增量续写".into() },
        );

        assert_eq!(state.streaming_text, "第一段流式增量续写");
        assert_eq!(build_feishu_cardkit_stream_markdown(&state), "第一段流式增量续写");
    }

    #[test]
    fn finalize_state_promotes_completed_segments_over_streaming_text() {
        let mut state = FeishuStreamingCardState::new("用户问题".into(), 10);
        state.streaming_text = "旧的流式文本".into();

        finalize_feishu_card_state(
            &mut state,
            Some(vec!["第一段最终回复".into(), "第二段最终回复".into()]),
            None,
            20,
        );

        assert_eq!(state.streaming_text, "");
        assert_eq!(state.completed_segments, vec!["第一段最终回复", "第二段最终回复"]);
        assert_eq!(state.final_text(), "第一段最终回复\n\n第二段最终回复");
    }
}
