use serde_json::json;

use super::card::{FeishuStreamingToolState, FeishuStreamingToolStatus};
use super::protocol::{
    Chat, EventBody, FEISHU_FRAME_TYPE_DATA, FEISHU_HEADER_MESSAGE_ID, FEISHU_HEADER_TYPE,
    FeishuFrame, FeishuHeader, FeishuServerClientConfig, FeishuWebsocketEndpoint,
    FeishuWebsocketEndpointResponse, Mention, Message, Sender, SenderId,
};
use super::*;

const FEISHU_MESSAGE_TYPE_EVENT: &str = "event";

fn sample_profile() -> ChannelProfile {
    ChannelProfile::new(
        "default",
        "默认飞书",
        ChannelTransport::Feishu,
        json!({
            "app_id": "cli_app",
            "app_secret": "secret",
            "base_url": "https://open.feishu.cn",
            "require_mention": true,
            "thread_mode": true
        }),
    )
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
    assert_eq!(decoded.data.expect("endpoint data").url, "wss://open.feishu.cn/ws?service_id=12");
}

fn extract_final_answer_from_turn(turn: &agent_runtime::TurnLifecycle) -> Option<String> {
    let assistant_blocks = extract_assistant_segments_from_turn(turn);

    if !assistant_blocks.is_empty() {
        return Some(assistant_blocks.join("\n\n"));
    }

    turn.assistant_message.clone().filter(|message| !message.trim().is_empty())
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
    profile.config["base_url"] = json!("https://open.feishu.cn");
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
    profile.config["base_url"] = json!("https://open.feishu.cn");

    assert_eq!(
        feishu_message_reactions_url(&profile, "om_message_1"),
        "https://open.feishu.cn/open-apis/im/v1/messages/om_message_1/reactions"
    );
}

#[test]
fn streaming_card_payload_renders_reasoning_answer_and_tool_status() {
    let state = FeishuStreamingCardState {
        user_message: "用户问题".into(),
        status: ChannelTurnStatus::Generating,
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
        element["tag"] == "collapsible_panel" && element["elements"][0]["content"] == "先分析上下文"
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
    profile.config["base_url"] = json!("https://open.feishu.cn");
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
            started_at_ms: 12,
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
            finished_at_ms: 22,
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
