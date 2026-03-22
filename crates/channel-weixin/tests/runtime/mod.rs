use std::sync::Arc;

use channel_bridge::{ChannelProfile, ChannelRuntimeHost, ChannelTransport};
use serde_json::json;

use crate::runtime::build_weixin_runtime_adapter;

#[derive(Clone)]
struct FakeHost;

#[async_trait::async_trait]
impl channel_bridge::ChannelBindingStore for FakeHost {
    async fn get_channel_binding(
        &self,
        _key: agent_store::ExternalConversationKey,
    ) -> Result<Option<agent_store::ChannelSessionBinding>, channel_bridge::ChannelBridgeError>
    {
        Ok(None)
    }

    async fn upsert_channel_binding(
        &self,
        _binding: agent_store::ChannelSessionBinding,
    ) -> Result<(), channel_bridge::ChannelBridgeError> {
        Ok(())
    }

    async fn record_channel_message_receipt(
        &self,
        _receipt: agent_store::ChannelMessageReceipt,
    ) -> Result<bool, channel_bridge::ChannelBridgeError> {
        Ok(true)
    }
}

#[async_trait::async_trait]
impl channel_bridge::ChannelSessionService for FakeHost {
    async fn session_exists(
        &self,
        _session_id: &str,
    ) -> Result<bool, channel_bridge::ChannelBridgeError> {
        Ok(true)
    }

    async fn create_session(
        &self,
        _title: String,
    ) -> Result<String, channel_bridge::ChannelBridgeError> {
        Ok("session-1".into())
    }

    async fn session_info(
        &self,
        _session_id: &str,
    ) -> Result<channel_bridge::ChannelSessionInfo, channel_bridge::ChannelBridgeError> {
        Ok(channel_bridge::ChannelSessionInfo { pressure_ratio: None })
    }

    async fn auto_compress_session(
        &self,
        _session_id: &str,
    ) -> Result<bool, channel_bridge::ChannelBridgeError> {
        Ok(false)
    }
}

#[async_trait::async_trait]
impl ChannelRuntimeHost for FakeHost {
    async fn submit_turn(&self, _session_id: String, _prompt: String) -> Result<String, String> {
        Ok("turn-1".into())
    }

    fn subscribe_runtime_events(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<channel_bridge::ChannelRuntimeEvent> {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        rx
    }
}

fn sample_profile() -> ChannelProfile {
    ChannelProfile::new(
        "default",
        "Default Weixin",
        ChannelTransport::Weixin,
        json!({
            "bot_token": "bot-token"
        }),
    )
}

#[test]
fn weixin_adapter_definition_uses_weixin_transport() {
    let host: Arc<dyn ChannelRuntimeHost> = Arc::new(FakeHost);
    let adapter = build_weixin_runtime_adapter(host);
    let definition = adapter.definition();

    assert_eq!(adapter.transport(), ChannelTransport::Weixin);
    assert_eq!(definition.transport, ChannelTransport::Weixin);
    assert_eq!(definition.label, "Weixin");
}

#[test]
fn weixin_adapter_fingerprint_changes_with_bot_token() {
    let host: Arc<dyn ChannelRuntimeHost> = Arc::new(FakeHost);
    let adapter = build_weixin_runtime_adapter(host);
    let profile = sample_profile();
    let mut modified = sample_profile();
    modified.config["bot_token"] = json!("bot-token-2");

    let left = adapter.fingerprint(&profile).expect("fingerprint");
    let right = adapter.fingerprint(&modified).expect("fingerprint");

    assert_ne!(left, right);
}

#[test]
fn weixin_adapter_rejects_missing_bot_token() {
    let host: Arc<dyn ChannelRuntimeHost> = Arc::new(FakeHost);
    let adapter = build_weixin_runtime_adapter(host);
    let mut profile = sample_profile();
    profile.config["bot_token"] = json!("");

    assert!(adapter.validate_config(&profile.config).is_err());
}

#[test]
fn weixin_adapter_accepts_legacy_url_fields() {
    let host: Arc<dyn ChannelRuntimeHost> = Arc::new(FakeHost);
    let adapter = build_weixin_runtime_adapter(host);
    let profile = ChannelProfile::new(
        "legacy",
        "Legacy Weixin",
        ChannelTransport::Weixin,
        json!({
            "bot_token": "bot-token",
            "base_url": "https://ilinkai.weixin.qq.com",
            "cdn_base_url": "https://novac2c.cdn.weixin.qq.com/c2c"
        }),
    );

    assert!(adapter.validate_config(&profile.config).is_ok());
}
