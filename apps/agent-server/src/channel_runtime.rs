use std::{sync::Arc, time::Duration};

use agent_store::{
    ChannelMessageReceipt, ChannelSessionBinding, ExternalConversationKey, FeishuMessageTarget,
};
use channel_registry::{ChannelProfile, ChannelRegistry, ChannelTransport};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    session_manager::{RuntimeWorkerError, SessionManagerHandle, read_lock},
    sse::SsePayload,
    state::AppState,
};

pub async fn sync_feishu_runtime(_state: &AppState) -> Result<(), String> {
    Ok(())
}

pub async fn handle_feishu_webhook(state: &AppState, payload: &[u8]) -> Result<Value, String> {
    if let Ok(challenge) = serde_json::from_slice::<WebhookChallenge>(payload)
        && challenge.kind == "url_verification"
    {
        return Ok(json!({ "challenge": challenge.challenge }));
    }

    let envelope: EventEnvelope =
        serde_json::from_slice(payload).map_err(|error| error.to_string())?;
    if envelope.header.event_type != "im.message.receive_v1" {
        return Ok(json!({ "ok": true }));
    }

    let registry = read_lock(&state.channel_registry_snapshot).clone();
    let Some(profile) = find_feishu_profile(&registry, &envelope.header.app_id) else {
        return Err(format!("未找到 app_id={} 对应的飞书 channel", envelope.header.app_id));
    };

    let deps = FeishuRuntimeDeps {
        store: state.store.clone(),
        session_manager: state.session_manager.clone(),
        broadcast_tx: state.broadcast_tx.clone(),
    };
    handle_event(&profile, &deps, envelope.event).await?;
    Ok(json!({ "ok": true }))
}

fn find_feishu_profile(registry: &ChannelRegistry, app_id: &str) -> Option<ChannelProfile> {
    registry
        .channels()
        .iter()
        .find(|profile| {
            profile.transport == ChannelTransport::Feishu
                && profile.enabled
                && profile.config.app_id == app_id
        })
        .cloned()
}

#[derive(Clone)]
struct FeishuRuntimeDeps {
    store: Arc<agent_store::AiaStore>,
    session_manager: SessionManagerHandle,
    broadcast_tx: tokio::sync::broadcast::Sender<SsePayload>,
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

    prepare_session_for_turn(&deps.session_manager, &session_id).await?;

    let reply_target = resolve_reply_target(profile, &event);
    let turn_prompt = build_turn_prompt(&prompt, &event);
    let reply = submit_turn_and_wait(deps, session_id, turn_prompt).await.unwrap_or_else(|error| {
        if error.contains("already running") || error.contains("正在") {
            "当前会话仍在处理中，请稍后再试。".into()
        } else {
            format!("处理消息失败：{error}")
        }
    });

    send_reply_message(profile, &reply_target, &reply, &event.message.message_id).await
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
struct WebhookChallenge {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
}

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    header: EventHeader,
    event: EventBody,
}

#[derive(Debug, Deserialize)]
struct EventHeader {
    event_type: String,
    app_id: String,
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

#[cfg(test)]
mod tests {
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
        let payload = serde_json::to_vec(&json!({
            "type": "url_verification",
            "challenge": "challenge-token"
        }))
        .expect("serialize challenge");
        let challenge: WebhookChallenge =
            serde_json::from_slice(&payload).expect("deserialize challenge");
        assert_eq!(challenge.kind, "url_verification");
        assert_eq!(challenge.challenge, "challenge-token");
    }
}
