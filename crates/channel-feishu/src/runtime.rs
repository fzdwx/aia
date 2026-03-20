mod card;
mod config;
mod protocol;
#[cfg(test)]
#[path = "../tests/runtime/mod.rs"]
mod tests;

use std::{sync::Arc, time::Duration};

use agent_core::ToolArgsSchema;
use agent_store::ExternalConversationKey;
use channel_bridge::{
    ChannelBindingStore, ChannelBridgeError, ChannelCurrentTurnSnapshot, ChannelRuntimeAdapter,
    ChannelRuntimeEvent, ChannelRuntimeHost, ChannelSessionService, ChannelTurnStatus,
    SupportedChannelDefinition, prepare_session_for_turn, record_channel_message_receipt,
    resolve_or_create_session,
};
use channel_bridge::{ChannelProfile, ChannelTransport};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{task::JoinHandle, time::MissedTickBehavior};
use tokio_tungstenite::{connect_async, tungstenite::Message as WebSocketMessage};

use card::{
    FeishuStreamingCardState, FeishuStreamingReplyMode, apply_stream_event_to_feishu_card_state,
    build_cardkit_stream_payload, build_feishu_card_payload, build_feishu_cardkit_shell,
    build_feishu_cardkit_stream_markdown, current_timestamp_ms, finalize_feishu_card_state,
    prepare_send_card_request, prepare_send_cardkit_request, prepare_send_message_request,
    update_feishu_card_state_from_snapshot,
};
pub use config::FeishuMessageTarget;
use config::{FeishuChannelConfig, feishu_config, parse_feishu_config};
use protocol::{
    EventBody, EventEnvelope, FEISHU_FRAME_TYPE_CONTROL, FEISHU_FRAME_TYPE_DATA,
    FEISHU_HEADER_BIZ_RT, FEISHU_HEADER_MESSAGE_ID, FEISHU_HEADER_SEQ, FEISHU_HEADER_SERVICE_ID,
    FEISHU_HEADER_SUM, FEISHU_HEADER_TYPE, FEISHU_MESSAGE_TYPE_PONG, FeishuCardKitCreateResponse,
    FeishuConnectionPolicy, FeishuFrame, FeishuHeader, FeishuHeaders, FeishuMessageResponse,
    FeishuP2pChatQueryResponse, FeishuReactionCreateResponse, FeishuServerClientConfig,
    FeishuSimpleResponse, FeishuWebsocketEndpoint, FeishuWebsocketEndpointResponse,
    FeishuWsResponse, Mention, PendingFrameBuffer, TenantAccessTokenResponse, TextContent,
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

#[derive(Clone)]
struct ChannelRuntimeDeps {
    host: Arc<dyn ChannelRuntimeHost>,
}

#[derive(Clone)]
struct FeishuChannelAdapter {
    deps: ChannelRuntimeDeps,
}

impl FeishuChannelAdapter {
    fn new(host: Arc<dyn ChannelRuntimeHost>) -> Self {
        Self { deps: ChannelRuntimeDeps { host } }
    }
}

impl ChannelRuntimeAdapter for FeishuChannelAdapter {
    fn transport(&self) -> ChannelTransport {
        ChannelTransport::Feishu
    }

    fn definition(&self) -> SupportedChannelDefinition {
        SupportedChannelDefinition {
            transport: ChannelTransport::Feishu,
            label: "Feishu".into(),
            description: Some("飞书长连接 channel".into()),
            config_schema: FeishuChannelConfig::schema().into_value(),
        }
    }

    fn validate_config(&self, config: &Value) -> Result<(), ChannelBridgeError> {
        parse_feishu_config(config).map(|_| ())
    }

    fn fingerprint(&self, profile: &ChannelProfile) -> Result<String, ChannelBridgeError> {
        let config = feishu_config(profile);
        Ok(format!(
            "{}|{}|{}|{}|{}|{}|{}",
            profile.id,
            profile.enabled,
            config.app_id,
            config.app_secret,
            config.base_url,
            config.require_mention,
            config.thread_mode,
        ))
    }

    fn spawn(&self, profile: ChannelProfile) -> Result<JoinHandle<()>, ChannelBridgeError> {
        self.validate_config(&profile.config)?;
        let deps = self.deps.clone();
        Ok(tokio::spawn(async move {
            run_feishu_long_connection(profile, deps).await;
        }))
    }
}

pub fn build_feishu_runtime_adapter(
    host: Arc<dyn ChannelRuntimeHost>,
) -> Arc<dyn ChannelRuntimeAdapter> {
    Arc::new(FeishuChannelAdapter::new(host))
}

async fn run_feishu_long_connection(profile: ChannelProfile, deps: ChannelRuntimeDeps) {
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
    deps: &ChannelRuntimeDeps,
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
    deps: &ChannelRuntimeDeps,
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
    deps: &ChannelRuntimeDeps,
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
    let config = feishu_config(profile);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!(
        "{}/{}",
        config.base_url.trim_end_matches('/'),
        FEISHU_WS_ENDPOINT_URI.trim_start_matches('/'),
    );
    let response = client
        .post(url)
        .header("locale", "zh")
        .json(&json!({
            "AppID": config.app_id,
            "AppSecret": config.app_secret,
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
    deps: &ChannelRuntimeDeps,
    event: EventBody,
) -> Result<(), String> {
    let config = feishu_config(profile);
    if event.sender.sender_type != "user" {
        return Ok(());
    }
    if event.message.message_type != "text" {
        return Ok(());
    }
    if config.require_mention
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
    let bindings: &dyn ChannelBindingStore = deps.host.as_ref();
    let sessions: &dyn ChannelSessionService = deps.host.as_ref();
    let session_id = resolve_or_create_session(
        bindings,
        sessions,
        conversation_key,
        build_session_title(profile, &event),
    )
    .await
    .map_err(|error| error.to_string())?;
    let request_uuid_seed = event.message.message_id.clone();
    let first_seen = record_channel_message_receipt(
        bindings,
        "feishu",
        &profile.id,
        &event.message.message_id,
        &session_id,
    )
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
    deps: ChannelRuntimeDeps,
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
    deps: &ChannelRuntimeDeps,
    session_id: &str,
    prompt: &str,
    reply_target: &FeishuMessageTarget,
    request_uuid_seed: &str,
) -> Result<(), String> {
    let sessions: &dyn ChannelSessionService = deps.host.as_ref();
    prepare_session_for_turn(sessions, session_id).await.map_err(|error| error.to_string())?;

    let mut rx = deps.host.subscribe_runtime_events();
    let expected_turn_id =
        deps.host.submit_turn(session_id.to_string(), prompt.to_string()).await?;

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
            .map_err(|_| "等待飞书流式回复超时".to_string())?
            .ok_or_else(|| "飞书运行时事件流已结束".to_string())?;

        match received {
            ChannelRuntimeEvent::CurrentTurnStarted { session_id: sid, current_turn }
                if sid == session_id && current_turn.turn_id == expected_turn_id =>
            {
                controller.update_from_snapshot(&current_turn);
                controller.flush(profile, false).await;
            }
            ChannelRuntimeEvent::Status { session_id: sid, turn_id, status }
                if sid == session_id && turn_id == expected_turn_id =>
            {
                controller.update_status(status);
                controller.flush(profile, false).await;
            }
            ChannelRuntimeEvent::Stream { session_id: sid, turn_id, event }
                if sid == session_id && turn_id == expected_turn_id =>
            {
                controller.apply_stream(&event);
                controller.flush(profile, false).await;
            }
            ChannelRuntimeEvent::TurnCompleted { session_id: sid, turn_id, turn }
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
            ChannelRuntimeEvent::Error { session_id: sid, turn_id, message }
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

    fn update_from_snapshot(&mut self, snapshot: &ChannelCurrentTurnSnapshot) {
        update_feishu_card_state_from_snapshot(&mut self.state, snapshot);
    }

    fn update_status(&mut self, status: ChannelTurnStatus) {
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
    let config = feishu_config(profile);
    match event.message.chat_type.as_str() {
        "p2p" => format!("Feishu DM · {} · {}", profile.name, event.sender.sender_id.open_id),
        _ if config.thread_mode
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
    let config = feishu_config(profile);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let base_url = config.base_url.trim_end_matches('/');
    let response = client
        .post(format!("{base_url}/open-apis/auth/v3/tenant_access_token/internal"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&json!({
            "app_id": config.app_id,
            "app_secret": config.app_secret,
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
    let config = feishu_config(profile);
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

    if config.thread_mode
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
        reply_in_thread: !is_p2p && feishu_config(profile).thread_mode,
    })
}

fn resolve_p2p_chat_id_from_event(event: &EventBody) -> Option<&str> {
    event.message.chat_id.as_deref().filter(|chat_id| !chat_id.is_empty()).or_else(|| {
        event.chat.as_ref().map(|chat| chat.chat_id.as_str()).filter(|chat_id| !chat_id.is_empty())
    })
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
