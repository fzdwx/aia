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
    session_manager::{RuntimeWorkerError, SessionManagerHandle, read_lock},
    sse::SsePayload,
    state::AppState,
};

const FEISHU_EVENT_KIND: &str = "im.message.receive_v1";
const FEISHU_WS_ENDPOINT_URI: &str = "/callback/ws/endpoint";
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
            .map(|(profile_id, worker)| (profile_id.clone(), worker.fingerprint.clone()))
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

    let reply_target = resolve_reply_target(profile, &event);
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
        let reply = async {
            prepare_session_for_turn(&deps.session_manager, &session_id).await?;
            submit_turn_and_wait(&deps, session_id.clone(), prompt).await
        }
        .await
        .unwrap_or_else(|error| {
            if error.contains("already running") || error.contains("正在") {
                "当前会话仍在处理中，请稍后再试。".into()
            } else {
                format!("处理消息失败：{error}")
            }
        });

        if let Err(error) =
            send_reply_message(&profile, &reply_target, &reply, &request_uuid_seed).await
        {
            eprintln!(
                "发送飞书异步回复失败 profile_id={} message_id={} error={error}",
                profile.id, request_uuid_seed
            );
        }
    });
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

async fn submit_turn_and_wait(
    deps: &FeishuRuntimeDeps,
    session_id: String,
    prompt: String,
) -> Result<String, String> {
    let mut rx = deps.broadcast_tx.subscribe();
    deps.session_manager
        .submit_turn(session_id.clone(), prompt)
        .map_err(runtime_error_to_string)?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let received = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| "等待飞书回复超时".to_string())
            .and_then(|result| result.map_err(|error| error.to_string()))?;
        match received {
            SsePayload::TurnCompleted { session_id: sid, turn } if sid == session_id => {
                if let Some(message) = turn.assistant_message {
                    return Ok(message);
                }
                if let Some(message) = turn.failure_message {
                    return Ok(message);
                }
                return Ok("已完成处理，但没有生成可发送的文本回复。".into());
            }
            SsePayload::Error { session_id: sid, message } if sid == session_id => {
                return Err(message);
            }
            _ => {}
        }
    }
}

fn runtime_error_to_string(error: RuntimeWorkerError) -> String {
    error.message
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
    let base_url = profile.config.base_url.trim_end_matches('/');
    let url = format!(
        "{base_url}/open-apis/im/v1/messages/{}/reply",
        target.reply_to_message_id.as_deref().unwrap_or_default()
    );
    let body = json!({
        "msg_type": "text",
        "content": json!({ "text": text }).to_string(),
        "reply_in_thread": target.reply_in_thread,
        "uuid": format!("aia-{}", request_uuid_seed),
    });
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {tenant_access_token}"))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&body)
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
    Ok(())
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

fn resolve_reply_target(profile: &ChannelProfile, event: &EventBody) -> FeishuMessageTarget {
    FeishuMessageTarget {
        receive_id: event
            .message
            .chat_id
            .clone()
            .unwrap_or_else(|| event.sender.sender_id.open_id.clone()),
        receive_id_type: if event.message.chat_type == "p2p" {
            "open_id".into()
        } else {
            "chat_id".into()
        },
        reply_to_message_id: Some(event.message.message_id.clone()),
        reply_in_thread: profile.config.thread_mode,
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
    existing: &HashMap<String, String>,
    desired_profiles: &[ChannelProfile],
) -> (Vec<String>, Vec<ChannelProfile>) {
    let desired_fingerprints = desired_profiles
        .iter()
        .map(|profile| (profile.id.clone(), profile_fingerprint(profile)))
        .collect::<HashMap<_, _>>();

    let stop_ids = existing
        .iter()
        .filter_map(|(profile_id, fingerprint)| match desired_fingerprints.get(profile_id) {
            Some(desired) if desired == fingerprint => None,
            _ => Some(profile_id.clone()),
        })
        .collect::<Vec<_>>();
    let start_profiles = desired_profiles
        .iter()
        .filter(|profile| existing.get(&profile.id) != Some(&profile_fingerprint(profile)))
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
            ("default".to_string(), "same".to_string()),
            ("legacy".to_string(), "old".to_string()),
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
}
