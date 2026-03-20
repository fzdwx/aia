use serde_json::json;

use super::{ParsedSseLine, StreamingTranscript};

#[test]
fn parse_json_line_ignores_non_data_prefix() {
    let mut transcript = StreamingTranscript::default();
    let parsed = transcript.parse_json_line("event: message").expect("parse succeeds");

    assert!(matches!(parsed, ParsedSseLine::Ignore));
    assert_eq!(transcript.into_response_body(), Some(String::new()));
}

#[test]
fn parse_json_line_records_done_and_invalid_json_lines() {
    let mut transcript = StreamingTranscript::default();

    let invalid =
        transcript.parse_json_line("data: {not-json}").expect("invalid json still ignored");
    let done = transcript.parse_json_line("data: [DONE]").expect("done parses");

    assert!(matches!(invalid, ParsedSseLine::Ignore));
    assert!(matches!(done, ParsedSseLine::Done));
    assert_eq!(transcript.into_response_body(), Some("data: {not-json}\ndata: [DONE]".to_string()));
}

#[test]
fn parse_json_line_extracts_json_event() {
    let mut transcript = StreamingTranscript::default();
    let parsed =
        transcript.parse_json_line(r#"data: {"type":"response.completed"}"#).expect("json parses");

    let ParsedSseLine::Json(event) = parsed else {
        panic!("expected json event");
    };
    assert_eq!(event, json!({ "type": "response.completed" }));
}
