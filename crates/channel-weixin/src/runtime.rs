#[cfg(test)]
#[path = "../tests/runtime/mod.rs"]
mod tests;

use std::{sync::Arc, time::Duration};

use agent_core::ToolArgsSchema;
use agent_store::ExternalConversationKey;
use channel_bridge::{
    ChannelBindingStore, ChannelBridgeError, ChannelProfile, ChannelRuntimeAdapter,
    ChannelRuntimeEvent, ChannelRuntimeHost, ChannelSessionService, ChannelTransport,
    SupportedChannelDefinition, prepare_session_for_turn, record_channel_message_receipt,
    resolve_or_create_session,
};
use md5::{Digest, Md5};
use serde_json::Value;
use tokio::task::JoinHandle;
use weixin_client::{
    InboundMessage, InboundMessageExt, SendTextRequest, TypingStatusRequest, WeixinClient,
};

use crate::config::{
    WeixinChannelConfig, parse_weixin_config, weixin_client_config, weixin_config,
};

#[derive(Clone)]
struct ChannelRuntimeDeps {
    host: Arc<dyn ChannelRuntimeHost>,
}

#[derive(Clone)]
struct WeixinChannelAdapter {
    deps: ChannelRuntimeDeps,
}

#[derive(Clone, Debug)]
struct WeixinReplyTarget {
    to_user_id: String,
    context_token: String,
    typing_user_id: String,
}

impl WeixinChannelAdapter {
    fn new(host: Arc<dyn ChannelRuntimeHost>) -> Self {
        Self { deps: ChannelRuntimeDeps { host } }
    }
}

impl ChannelRuntimeAdapter for WeixinChannelAdapter {
    fn transport(&self) -> ChannelTransport {
        ChannelTransport::Weixin
    }

    fn definition(&self) -> SupportedChannelDefinition {
        SupportedChannelDefinition {
            transport: ChannelTransport::Weixin,
            label: "Weixin".into(),
            description: Some("Weixin / iLink long-polling channel".into()),
            config_schema: WeixinChannelConfig::schema().into_value(),
        }
    }

    fn validate_config(&self, config: &Value) -> Result<(), ChannelBridgeError> {
        parse_weixin_config(config).map(|_| ())
    }

    fn fingerprint(&self, profile: &ChannelProfile) -> Result<String, ChannelBridgeError> {
        let config = weixin_config(profile);
        Ok(format!("{}|{}|{}", profile.id, profile.enabled, config.bot_token,))
    }

    fn spawn(&self, profile: ChannelProfile) -> Result<JoinHandle<()>, ChannelBridgeError> {
        self.validate_config(&profile.config)?;
        let deps = self.deps.clone();
        Ok(tokio::spawn(async move {
            run_weixin_long_poll(profile, deps).await;
        }))
    }
}

pub fn build_weixin_runtime_adapter(
    host: Arc<dyn ChannelRuntimeHost>,
) -> Arc<dyn ChannelRuntimeAdapter> {
    Arc::new(WeixinChannelAdapter::new(host))
}

async fn run_weixin_long_poll(profile: ChannelProfile, deps: ChannelRuntimeDeps) {
    let mut cursor = String::new();
    let client = match build_client(&profile) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("微信 long poll 初始化失败 profile_id={} error={error}", profile.id);
            return;
        }
    };

    loop {
        match client.get_updates(Some(&cursor)).await {
            Ok(response) => {
                if !response.get_updates_buf.trim().is_empty() {
                    cursor = response.get_updates_buf;
                }
                for message in response.msgs {
                    if let Err(error) =
                        handle_inbound_message(&profile, &deps, &client, message).await
                    {
                        eprintln!("微信消息处理失败 profile_id={} error={error}", profile.id);
                    }
                }
            }
            Err(error) => {
                eprintln!("微信轮询失败 profile_id={} error={error}", profile.id);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

fn build_client(profile: &ChannelProfile) -> Result<WeixinClient, ChannelBridgeError> {
    let config = weixin_client_config(profile);
    WeixinClient::new(config).map_err(|error| ChannelBridgeError::new(error.to_string()))
}

async fn handle_inbound_message(
    profile: &ChannelProfile,
    deps: &ChannelRuntimeDeps,
    client: &WeixinClient,
    message: InboundMessage,
) -> Result<(), String> {
    if !message.should_handle_inbound_message() {
        return Ok(());
    }

    let prompt = message.extract_inbound_text();
    if prompt.trim().is_empty() {
        if message.has_unsupported_inbound_media() {
            send_plain_text_reply(
                client,
                &build_reply_target(&message).map_err(|error| error.to_string())?,
                "Only text messages and voice transcripts are supported right now.",
            )
            .await?;
        }
        return Ok(());
    }

    let bindings: &dyn ChannelBindingStore = deps.host.as_ref();
    let sessions: &dyn ChannelSessionService = deps.host.as_ref();
    let conversation_key = resolve_conversation_key(profile, &message);
    let session_id = resolve_or_create_session(
        bindings,
        sessions,
        conversation_key,
        build_session_title(profile, &message),
    )
    .await
    .map_err(|error| error.to_string())?;

    let external_message_id = external_message_id(&message).map_err(|error| error.to_string())?;
    let first_seen = record_channel_message_receipt(
        bindings,
        "weixin",
        &profile.id,
        &external_message_id,
        &session_id,
    )
    .await
    .map_err(|error| error.to_string())?;
    if !first_seen {
        return Ok(());
    }

    let reply_target = build_reply_target(&message).map_err(|error| error.to_string())?;
    spawn_turn_reply_job(
        profile.clone(),
        deps.clone(),
        client.clone(),
        session_id,
        build_turn_prompt(&prompt, &message),
        reply_target,
    );
    Ok(())
}

fn spawn_turn_reply_job(
    profile: ChannelProfile,
    deps: ChannelRuntimeDeps,
    client: WeixinClient,
    session_id: String,
    prompt: String,
    reply_target: WeixinReplyTarget,
) {
    tokio::spawn(async move {
        if let Err(error) = stream_turn_to_weixin_reply(
            &profile,
            &deps,
            &client,
            &session_id,
            &prompt,
            &reply_target,
        )
        .await
        {
            eprintln!(
                "发送微信回复失败 profile_id={} session_id={} error={error}",
                profile.id, session_id
            );
        }
    });
}

async fn stream_turn_to_weixin_reply(
    profile: &ChannelProfile,
    deps: &ChannelRuntimeDeps,
    client: &WeixinClient,
    session_id: &str,
    prompt: &str,
    reply_target: &WeixinReplyTarget,
) -> Result<(), String> {
    let sessions: &dyn ChannelSessionService = deps.host.as_ref();
    prepare_session_for_turn(sessions, session_id).await.map_err(|error| error.to_string())?;

    let typing_ticket = client
        .get_typing_ticket(&reply_target.typing_user_id, Some(&reply_target.context_token))
        .await
        .ok()
        .filter(|value| !value.trim().is_empty());
    if let Some(ticket) = typing_ticket.as_deref() {
        let _ = client
            .send_typing_status(TypingStatusRequest {
                ilink_user_id: reply_target.typing_user_id.clone(),
                typing_ticket: ticket.to_owned(),
                status: 1,
            })
            .await;
    }

    let result = async {
        let mut rx = deps.host.subscribe_runtime_events();
        let expected_turn_id =
            deps.host.submit_turn(session_id.to_string(), prompt.to_string()).await?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let received = tokio::time::timeout(remaining, rx.recv())
                .await
                .map_err(|_| "等待微信回复超时".to_string())?
                .ok_or_else(|| "微信运行时事件流已结束".to_string())?;

            match received {
                ChannelRuntimeEvent::TurnCompleted { session_id: sid, turn_id, turn }
                    if sid == session_id && turn_id == expected_turn_id =>
                {
                    let reply = build_turn_reply(&turn);
                    send_plain_text_reply(client, reply_target, &reply).await?;
                    return Ok(());
                }
                ChannelRuntimeEvent::Error { session_id: sid, turn_id, message }
                    if sid == session_id
                        && turn_id.as_deref() == Some(expected_turn_id.as_str()) =>
                {
                    let reply = if message.contains("already running") || message.contains("正在")
                    {
                        "The current session is still running. Please try again in a moment."
                            .to_owned()
                    } else {
                        format!("Message processing failed: {message}")
                    };
                    send_plain_text_reply(client, reply_target, &reply).await?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    .await;

    if let Some(ticket) = typing_ticket.as_deref() {
        let _ = client
            .send_typing_status(TypingStatusRequest {
                ilink_user_id: reply_target.typing_user_id.clone(),
                typing_ticket: ticket.to_owned(),
                status: 0,
            })
            .await;
    }

    let _ = profile;
    result
}

async fn send_plain_text_reply(
    client: &WeixinClient,
    reply_target: &WeixinReplyTarget,
    text: &str,
) -> Result<(), String> {
    let chunks = WeixinClient::split_text_for_weixin(text, 4000);
    let chunks = if chunks.is_empty() { vec![String::new()] } else { chunks };

    for chunk in chunks {
        client
            .send_text(SendTextRequest::new(
                reply_target.to_user_id.clone(),
                reply_target.context_token.clone(),
                chunk,
            ))
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn resolve_conversation_key(
    profile: &ChannelProfile,
    message: &InboundMessage,
) -> ExternalConversationKey {
    ExternalConversationKey {
        channel_kind: "weixin".into(),
        profile_id: profile.id.clone(),
        scope: "user".into(),
        conversation_key: message.from_user_id.trim().to_owned(),
    }
}

fn build_session_title(profile: &ChannelProfile, message: &InboundMessage) -> String {
    format!("Weixin DM · {} · {}", profile.name, message.from_user_id.trim())
}

fn build_turn_prompt(prompt: &str, message: &InboundMessage) -> String {
    format!(
        "Incoming Weixin message\nSender: {}\nMessage:\n{}",
        message.from_user_id.trim(),
        prompt
    )
}

fn build_reply_target(message: &InboundMessage) -> Result<WeixinReplyTarget, ChannelBridgeError> {
    let to_user_id = message.from_user_id.trim();
    let context_token = message.context_token.trim();
    if to_user_id.is_empty() {
        return Err(ChannelBridgeError::new("weixin message missing from_user_id"));
    }
    if context_token.is_empty() {
        return Err(ChannelBridgeError::new("weixin message missing context_token"));
    }

    Ok(WeixinReplyTarget {
        to_user_id: to_user_id.to_owned(),
        context_token: context_token.to_owned(),
        typing_user_id: to_user_id.to_owned(),
    })
}

fn external_message_id(message: &InboundMessage) -> Result<String, ChannelBridgeError> {
    let explicit = message
        .message_id
        .map(|value| value.to_string())
        .or_else(|| {
            message
                .client_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            let token = message.context_token.trim();
            if token.is_empty() { None } else { Some(token.to_owned()) }
        });
    if let Some(id) = explicit {
        return Ok(id);
    }

    let payload = serde_json::to_vec(message).map_err(|error| {
        ChannelBridgeError::new(format!("serialize weixin message failed: {error}"))
    })?;
    let digest = Md5::digest(payload);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn build_turn_reply(turn: &agent_runtime::TurnLifecycle) -> String {
    let assistant = extract_assistant_segments_from_turn(turn);
    if !assistant.is_empty() {
        return assistant.join("\n\n");
    }
    if let Some(message) = turn.assistant_message.as_ref().filter(|value| !value.trim().is_empty())
    {
        return message.clone();
    }
    if let Some(message) = turn.failure_message.as_ref().filter(|value| !value.trim().is_empty()) {
        return format!("Message processing failed: {message}");
    }
    "No response generated.".into()
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
