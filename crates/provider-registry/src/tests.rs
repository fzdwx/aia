use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    ModelConfig, ModelLimit, ProviderKind, ProviderProfile, ProviderRegistry, default_registry_path,
};

fn temp_file(name: &str) -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("时间有效").as_nanos();
    std::env::temp_dir().join(format!("aia-{name}-{suffix}.json"))
}

#[test]
fn 默认存储路径位于项目隐藏目录() {
    assert_eq!(default_registry_path(), PathBuf::from(".aia/providers.json"));
}

#[test]
fn 可保存并重新载入注册表() {
    let path = temp_file("provider-registry");
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-4.1-mini",
    ));
    registry.set_active("main").expect("设置活动 provider 成功");

    registry.save(&path).expect("保存成功");
    let restored = ProviderRegistry::load_or_default(&path).expect("加载成功");

    assert_eq!(restored.providers().len(), 1);
    assert_eq!(restored.active_provider().map(|provider| provider.name.as_str()), Some("main"));

    let _ = fs::remove_file(path);
}

#[test]
fn 同名_provider_会被更新而不是重复追加() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-4.1-mini",
    ));
    registry.upsert(ProviderProfile::openai_responses(
        "main",
        "https://example.com/v1",
        "secret-2",
        "gpt-4.1",
    ));

    assert_eq!(registry.providers().len(), 1);
    assert_eq!(registry.providers()[0].base_url, "https://example.com/v1");
    assert_eq!(registry.providers()[0].active_model_id(), Some("gpt-4.1"));
    assert!(registry.providers()[0].has_model("gpt-4.1"));
}

#[test]
fn 设置不存在的活动_provider_会报错() {
    let mut registry = ProviderRegistry::default();

    let error = registry.set_active("missing").expect_err("应当失败");

    assert!(error.to_string().contains("不存在"));
}

#[test]
fn 可构造_openai_兼容聊天补全_provider() {
    let provider = ProviderProfile::openai_chat_completions(
        "compat",
        "http://127.0.0.1:8000/v1",
        "secret",
        "minum-security-llm",
    );

    assert_eq!(provider.kind, ProviderKind::OpenAiChatCompletions);
    assert_eq!(provider.name, "compat");
    assert_eq!(provider.base_url, "http://127.0.0.1:8000/v1");
    assert_eq!(provider.active_model_id(), Some("minum-security-llm"));
}

#[test]
fn 模型_limit_可保存并重新载入() {
    let path = temp_file("provider-registry-limit");
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile {
        name: "main".into(),
        kind: ProviderKind::OpenAiResponses,
        base_url: "https://api.openai.com/v1".into(),
        api_key: "secret".into(),
        models: vec![ModelConfig {
            id: "gpt-4.1".into(),
            display_name: Some("GPT-4.1".into()),
            limit: Some(ModelLimit { context: Some(200_000), output: Some(131_072) }),
            default_temperature: Some(0.2),
            supports_reasoning: true,
            reasoning_effort: Some("medium".into()),
        }],
        active_model: Some("gpt-4.1".into()),
    });

    registry.save(&path).expect("保存成功");
    let restored = ProviderRegistry::load_or_default(&path).expect("加载成功");

    assert_eq!(
        restored.providers()[0].models[0].limit,
        Some(ModelLimit { context: Some(200_000), output: Some(131_072) })
    );

    let _ = fs::remove_file(path);
}

#[test]
fn 可兼容旧版单_model_格式() {
    let registry: ProviderRegistry = serde_json::from_value(serde_json::json!({
        "providers": [
            {
                "name": "legacy",
                "kind": "OpenAiResponses",
                "base_url": "https://api.openai.com/v1",
                "api_key": "secret",
                "model": "gpt-4.1-mini"
            }
        ],
        "active_provider": "legacy"
    }))
    .expect("旧格式应可恢复");

    assert_eq!(registry.providers().len(), 1);
    assert!(registry.providers()[0].has_model("gpt-4.1-mini"));
    assert_eq!(registry.providers()[0].active_model_id(), Some("gpt-4.1-mini"));
}

#[test]
fn 删除活动_provider_后会回退到下一个() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile::openai_responses(
        "main",
        "https://api.openai.com/v1",
        "secret",
        "gpt-4.1-mini",
    ));
    registry.upsert(ProviderProfile::openai_chat_completions(
        "backup",
        "http://127.0.0.1:8000/v1",
        "secret",
        "minum-security-llm",
    ));
    registry.set_active("main").expect("设置活动 provider 成功");

    registry.remove("main").expect("删除成功");

    assert_eq!(registry.active_provider().map(|provider| provider.name.as_str()), Some("backup"));
}

#[test]
fn 无效_active_model_会在写入时回退到首个模型() {
    let mut registry = ProviderRegistry::default();
    registry.upsert(ProviderProfile {
        name: "main".into(),
        kind: ProviderKind::OpenAiResponses,
        base_url: "https://api.openai.com/v1".into(),
        api_key: "secret".into(),
        models: vec![crate::ModelConfig::new("gpt-4.1-mini")],
        active_model: Some("missing".into()),
    });

    assert_eq!(registry.providers()[0].active_model_id(), Some("gpt-4.1-mini"));
}
