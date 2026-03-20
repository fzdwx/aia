use std::sync::Mutex;

use super::*;

struct FakeAdapter {
    transport: ChannelTransport,
    spawned: Mutex<Vec<String>>,
}

impl FakeAdapter {
    fn new(transport: ChannelTransport) -> Self {
        Self { transport, spawned: Mutex::new(Vec::new()) }
    }
}

impl ChannelRuntimeAdapter for FakeAdapter {
    fn transport(&self) -> ChannelTransport {
        self.transport.clone()
    }

    fn definition(&self) -> SupportedChannelDefinition {
        SupportedChannelDefinition {
            transport: self.transport(),
            label: "Fake".into(),
            description: None,
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false,
            }),
        }
    }

    fn validate_config(&self, _config: &Value) -> Result<(), ChannelBridgeError> {
        Ok(())
    }

    fn fingerprint(&self, profile: &ChannelProfile) -> Result<String, ChannelBridgeError> {
        let base_url = profile.config.get("base_url").and_then(Value::as_str).unwrap_or("");
        Ok(format!("{}:{}:{}", profile.id, profile.enabled, base_url))
    }

    fn spawn(&self, profile: ChannelProfile) -> Result<JoinHandle<()>, ChannelBridgeError> {
        self.spawned
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(profile.id.clone());
        Ok(tokio::spawn(async {}))
    }
}

fn sample_profile(id: &str) -> ChannelProfile {
    ChannelProfile::new(
        id,
        "默认飞书",
        ChannelTransport::Feishu,
        serde_json::json!({
            "app_id": "app",
            "app_secret": "secret",
            "base_url": "https://open.feishu.cn",
            "require_mention": true,
            "thread_mode": true
        }),
    )
}

#[test]
fn sync_errors_when_transport_has_no_adapter() {
    let mut supervisor = ChannelRuntimeSupervisor::new(ChannelAdapterCatalog::new());

    let error =
        supervisor.sync(vec![sample_profile("default")]).expect_err("missing adapter should fail");

    assert!(error.to_string().contains("missing channel runtime adapter"));
}

#[test]
fn reconcile_runtime_workers_restarts_changed_profiles() {
    let desired_profile = sample_profile("same");
    let desired = HashMap::from([(
        "same".to_string(),
        DesiredWorker {
            adapter: Arc::new(FakeAdapter::new(ChannelTransport::Feishu)),
            fingerprint: "same:new".to_string(),
            profile: desired_profile,
        },
    )]);
    let existing = HashMap::from([(
        "same".to_string(),
        RuntimeWorkerState { fingerprint: "same:old".to_string(), finished: false },
    )]);

    let (stop_ids, start_ids) = reconcile_runtime_workers(&existing, &desired);

    assert_eq!(stop_ids, vec!["same"]);
    assert_eq!(start_ids, vec!["same"]);
}

#[tokio::test(flavor = "current_thread")]
async fn sync_spawns_enabled_profile_once_when_fingerprint_matches() {
    let adapter = Arc::new(FakeAdapter::new(ChannelTransport::Feishu));
    let mut registry = ChannelAdapterCatalog::new();
    registry.register(adapter.clone());
    let mut supervisor = ChannelRuntimeSupervisor::new(registry);
    let profile = sample_profile("default");

    supervisor.sync(vec![profile.clone()]).expect("first sync should succeed");
    supervisor.sync(vec![profile]).expect("second sync should succeed");
    tokio::task::yield_now().await;

    let spawned = adapter.spawned.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(spawned, vec!["default"]);
}

#[test]
fn registry_returns_supported_definitions() {
    let adapter = Arc::new(FakeAdapter::new(ChannelTransport::Feishu));
    let mut registry = ChannelAdapterCatalog::new();
    registry.register(adapter);

    let definitions = registry.definitions();

    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].transport, ChannelTransport::Feishu);
}

#[test]
fn channel_transport_serializes_to_wire_literal() {
    let value =
        serde_json::to_value(ChannelTransport::Feishu).expect("channel transport should serialize");

    assert_eq!(value, serde_json::json!("feishu"));
    assert_eq!(ChannelTransport::Feishu.to_string(), "feishu");
}
