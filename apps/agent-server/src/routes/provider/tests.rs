use provider_registry::{ModelConfig, ModelLimit, ProviderKind, ProviderProfile};

use super::{
    ModelConfigDto, ModelLimitDto,
    handlers::{parse_provider_kind, provider_info_from_snapshot, provider_list_item},
};

#[test]
fn model_config_dto_round_trip_preserves_limit() {
    let dto = ModelConfigDto {
        id: "gpt-4.1".into(),
        display_name: Some("GPT-4.1".into()),
        limit: Some(ModelLimitDto { context: Some(200_000), output: Some(131_072) }),
        default_temperature: Some(0.2),
        supports_reasoning: true,
        reasoning_effort: Some("medium".into()),
    };

    let model = ModelConfig::from(dto.clone());
    assert_eq!(model.limit, Some(ModelLimit { context: Some(200_000), output: Some(131_072) }));

    let round_trip = ModelConfigDto::from(&model);
    assert_eq!(round_trip.limit, dto.limit);
}

#[test]
fn parse_provider_kind_accepts_known_protocols() {
    assert!(matches!(parse_provider_kind("openai-responses"), Ok(ProviderKind::OpenAiResponses)));
    assert!(matches!(
        parse_provider_kind("openai-chat-completions"),
        Ok(ProviderKind::OpenAiChatCompletions)
    ));
}

#[test]
fn parse_provider_kind_rejects_unknown_protocol() {
    let response =
        parse_provider_kind("unknown-protocol").expect_err("unknown protocol should fail");
    assert_eq!(response.0, axum::http::StatusCode::BAD_REQUEST);
}

#[test]
fn provider_info_from_snapshot_projects_fields() {
    let info = provider_info_from_snapshot(&crate::session_manager::ProviderInfoSnapshot {
        name: "rayin".into(),
        model: "gpt-5.4".into(),
        connected: true,
    });

    assert_eq!(info.name, "rayin");
    assert_eq!(info.model, "gpt-5.4");
    assert!(info.connected);
}

#[test]
fn provider_list_item_marks_active_provider() {
    let profile = ProviderProfile {
        name: "rayin".into(),
        kind: ProviderKind::OpenAiResponses,
        base_url: "https://example.com".into(),
        api_key: "secret".into(),
        models: vec![ModelConfig {
            id: "gpt-5.4".into(),
            display_name: Some("GPT-5.4".into()),
            limit: Some(ModelLimit { context: Some(200_000), output: Some(8_192) }),
            default_temperature: Some(0.2),
            supports_reasoning: true,
            reasoning_effort: Some("medium".into()),
        }],
        active_model: Some("gpt-5.4".into()),
    };

    let item = provider_list_item(&profile, Some("rayin"));
    assert!(item.active);
    assert_eq!(item.kind, "openai-responses");
    assert_eq!(item.models.len(), 1);
}
