use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::ModelRef;

use crate::{
    AdapterKind, CredentialRef, ModelConfig, ModelLimit, ProviderAccount, ProviderEndpoint,
    ProviderRegistry, default_registry_path,
};

#[test]
fn 默认存储路径位于项目隐藏目录() {
    assert_eq!(default_registry_path(), aia_config::default_registry_path());
}

#[test]
fn 同名_provider_会被更新而不是重复追加() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-4.1-mini",
    ));
    registry.upsert(ProviderAccount::openai_responses(
        "main",
        "https://example.com/v1",
        "secret-2",
        "gpt-4.1",
    ));

    assert_eq!(registry.providers().len(), 1);
    assert_eq!(registry.providers()[0].endpoint.base_url, "https://example.com/v1");
    assert_eq!(registry.providers()[0].default_model_id(), Some("gpt-4.1"));
    assert!(registry.providers()[0].has_model("gpt-4.1"));
}

#[test]
fn 可构造_openai_兼容聊天补全_provider() {
    let provider = ProviderAccount::openai_chat_completions(
        "compat",
        "http://127.0.0.1:8000/v1",
        "secret",
        "minum-security-llm",
    );

    assert_eq!(provider.adapter, AdapterKind::OpenAiChatCompletions);
    assert_eq!(provider.id, "compat");
    assert_eq!(provider.endpoint.base_url, "http://127.0.0.1:8000/v1");
    assert_eq!(provider.default_model_id(), Some("minum-security-llm"));
}

#[test]
fn 删除_provider_后会从列表移除() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-4.1-mini",
    ));
    registry.upsert(ProviderAccount::openai_chat_completions(
        "backup",
        "http://127.0.0.1:8000/v1",
        "secret",
        "minum-security-llm",
    ));

    registry.remove("main").expect("删除成功");

    assert_eq!(registry.providers().len(), 1);
    assert_eq!(registry.first_provider().map(|p| p.id.as_str()), Some("backup"));
}

#[test]
fn 模型_limit_仍保留在领域模型里() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount {
        id: "main".into(),
        label: "main".into(),
        adapter: AdapterKind::OpenAiResponses,
        endpoint: ProviderEndpoint { base_url: "https://api.openai.com/v1".into() },
        credential: CredentialRef::api_key("secret"),
        models: vec![ModelConfig {
            id: "gpt-4.1".into(),
            display_name: Some("GPT-4.1".into()),
            limit: Some(ModelLimit { context: Some(200_000), output: Some(131_072) }),
            default_temperature: Some(0.2),
            supports_reasoning: true,
        }],
    });

    assert_eq!(
        registry.providers()[0].models[0].limit,
        Some(ModelLimit { context: Some(200_000), output: Some(131_072) })
    );
}

#[test]
fn provider_model_config_limit_复用_agent_core_共享类型() {
    let shared_limit = agent_core::ModelLimit { context: Some(128_000), output: Some(16_384) };

    let model = ModelConfig {
        id: "gpt-5-mini".into(),
        display_name: Some("GPT-5 Mini".into()),
        limit: Some(shared_limit.clone()),
        default_temperature: None,
        supports_reasoning: true,
    };

    assert_eq!(model.limit, Some(shared_limit));
}

#[test]
fn registry_can_resolve_model_ref() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-5-mini",
    ));

    let resolved = registry
        .resolve_model(&ModelRef::new("main", "gpt-5-mini"))
        .expect("model ref should resolve");

    assert_eq!(resolved.model_ref, ModelRef::new("main", "gpt-5-mini"));
    assert_eq!(resolved.base_url, "https://api.openai.com/v1");
    assert_eq!(resolved.model.id, "gpt-5-mini");
}

#[test]
fn first_model_ref_uses_first_provider_default_model() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderAccount::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-5-mini",
    ));

    assert_eq!(registry.first_model_ref(), Some(ModelRef::new("main", "gpt-5-mini")));
}

#[test]
fn 时间辅助在当前环境可用() {
    let _ = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
}
