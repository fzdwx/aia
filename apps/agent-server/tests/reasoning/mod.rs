use crate::reasoning::ReasoningEffort;

#[test]
fn parse_optional_accepts_known_literals() {
    assert_eq!(
        ReasoningEffort::parse_optional(Some("high")).expect("value should parse"),
        Some(ReasoningEffort::High)
    );
    assert_eq!(ReasoningEffort::parse_optional(None).expect("empty value should parse"), None);
}

#[test]
fn parse_optional_rejects_unknown_literals() {
    assert_eq!(
        ReasoningEffort::parse_optional(Some("turbo")).expect_err("unknown value should fail"),
        "invalid reasoning_effort: turbo"
    );
}

#[test]
fn normalize_for_model_drops_invalid_or_unsupported_values() {
    assert_eq!(ReasoningEffort::normalize_for_model(Some("high".into()), false), None);
    assert_eq!(ReasoningEffort::normalize_for_model(Some("turbo".into()), true), None);
    assert_eq!(
        ReasoningEffort::normalize_for_model(Some("high".into()), true),
        Some(ReasoningEffort::High)
    );
}
