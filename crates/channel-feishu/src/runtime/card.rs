use std::time::{SystemTime, UNIX_EPOCH};

use channel_bridge::{ChannelCurrentTurnSnapshot, ChannelProfile, ChannelTurnStatus};
use serde_json::{Value, json};

use super::{
    FEISHU_CARDKIT_STREAMING_ELEMENT_ID, FEISHU_MESSAGES_URI, FeishuMessageTarget, feishu_config,
};

pub(super) struct PreparedFeishuSendRequest {
    pub(super) url: String,
    pub(super) body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FeishuStreamingReplyMode {
    CardKit { card_id: String, message_id: String, sequence: u64 },
    InteractivePatch { message_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FeishuStreamingToolStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FeishuStreamingToolState {
    pub(super) invocation_id: String,
    pub(super) tool_name: String,
    pub(super) status: FeishuStreamingToolStatus,
    pub(super) output: String,
    pub(super) failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FeishuStreamingCardState {
    pub(super) user_message: String,
    pub(super) status: ChannelTurnStatus,
    pub(super) reasoning: String,
    pub(super) streaming_text: String,
    pub(super) completed_segments: Vec<String>,
    pub(super) tools: Vec<FeishuStreamingToolState>,
    pub(super) started_at_ms: u64,
    pub(super) finished_at_ms: Option<u64>,
    pub(super) failed: bool,
}

impl FeishuStreamingCardState {
    pub(super) fn new(user_message: String, started_at_ms: u64) -> Self {
        Self {
            user_message,
            status: ChannelTurnStatus::Waiting,
            reasoning: String::new(),
            streaming_text: String::new(),
            completed_segments: Vec::new(),
            tools: Vec::new(),
            started_at_ms,
            finished_at_ms: None,
            failed: false,
        }
    }

    pub(super) fn final_text(&self) -> String {
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

pub(super) fn build_feishu_card_payload(state: &FeishuStreamingCardState) -> Value {
    let mut elements = Vec::<Value>::new();

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

pub(super) fn build_feishu_cardkit_shell() -> Value {
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

pub(super) fn build_feishu_cardkit_stream_markdown(state: &FeishuStreamingCardState) -> String {
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

pub(super) fn build_cardkit_stream_payload(content: &str, sequence: u64) -> Value {
    json!({
        "content": content,
        "sequence": sequence,
    })
}

pub(super) fn apply_stream_event_to_feishu_card_state(
    state: &mut FeishuStreamingCardState,
    event: &agent_core::StreamEvent,
) {
    match event {
        agent_core::StreamEvent::ThinkingDelta { text } => {
            state.reasoning.push_str(text);
            state.status = ChannelTurnStatus::Thinking;
        }
        agent_core::StreamEvent::TextDelta { text } => {
            state.streaming_text.push_str(text);
            state.status = ChannelTurnStatus::Generating;
        }
        agent_core::StreamEvent::ToolCallDetected { invocation_id, tool_name, .. }
        | agent_core::StreamEvent::ToolCallArgumentsDelta { invocation_id, tool_name, .. }
        | agent_core::StreamEvent::ToolCallReady {
            call: agent_core::ToolCall { invocation_id, tool_name, .. },
        }
        | agent_core::StreamEvent::ToolCallStarted { invocation_id, tool_name, .. } => {
            let tool = find_or_insert_tool_state(state, invocation_id, tool_name);
            tool.status = FeishuStreamingToolStatus::Running;
            state.status = ChannelTurnStatus::Working;
        }
        agent_core::StreamEvent::ToolOutputDelta { invocation_id, text, .. } => {
            if let Some(tool) =
                state.tools.iter_mut().find(|tool| tool.invocation_id == *invocation_id)
            {
                tool.output.push_str(text);
            }
            state.status = ChannelTurnStatus::Working;
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
            state.status = ChannelTurnStatus::Working;
        }
        agent_core::StreamEvent::Done => {
            state.finished_at_ms = Some(current_timestamp_ms());
        }
        agent_core::StreamEvent::Retrying { .. } | agent_core::StreamEvent::Log { .. } => {}
    }
}

pub(super) fn update_feishu_card_state_from_snapshot(
    state: &mut FeishuStreamingCardState,
    snapshot: &ChannelCurrentTurnSnapshot,
) {
    state.user_message = snapshot.user_message.clone();
    state.status = snapshot.status.clone();
}

pub(super) fn finalize_feishu_card_state(
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
    state.status =
        if state.failed { ChannelTurnStatus::Cancelled } else { ChannelTurnStatus::Generating };
}

pub(super) fn current_timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

pub(super) fn prepare_send_message_request(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    text: &str,
    request_uuid_seed: &str,
) -> PreparedFeishuSendRequest {
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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

pub(super) fn prepare_send_card_request(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    card: &Value,
    request_uuid_seed: &str,
) -> PreparedFeishuSendRequest {
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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

pub(super) fn prepare_send_cardkit_request(
    profile: &ChannelProfile,
    target: &FeishuMessageTarget,
    card_id: &str,
    request_uuid_seed: &str,
) -> PreparedFeishuSendRequest {
    let config = feishu_config(profile);
    let base_url = config.base_url.trim_end_matches('/').to_string();
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
