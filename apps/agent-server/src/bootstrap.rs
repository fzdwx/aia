use std::sync::{Arc, RwLock};

use agent_store::{AiaStore, SessionRecord, generate_session_id};
use channel_registry::ChannelRegistry;
use provider_registry::ProviderRegistry;

use crate::{
    channel_runtime::sync_feishu_runtime,
    model::{ProviderLaunchChoice, build_model_from_selection},
    session_manager::{ProviderInfoSnapshot, SessionManagerConfig, spawn_session_manager},
    state::AppState,
};

pub fn build_server_user_agent() -> String {
    aia_config::build_user_agent(aia_config::APP_NAME, env!("CARGO_PKG_VERSION"))
}

#[derive(Debug)]
pub struct ServerInitError {
    step: &'static str,
    message: String,
}

impl ServerInitError {
    pub fn new(step: &'static str, message: impl Into<String>) -> Self {
        Self { step, message: message.into() }
    }
}

impl std::fmt::Display for ServerInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{step}失败: {message}", step = self.step, message = self.message)
    }
}

impl std::error::Error for ServerInitError {}

pub async fn bootstrap_state() -> Result<Arc<AppState>, ServerInitError> {
    let registry_path = provider_registry::default_registry_path();
    let channel_registry_path = channel_registry::default_registry_path();
    let aia_store_path = aia_config::store_path_from_registry_path(&registry_path);
    let sessions_dir = aia_config::sessions_dir_from_registry_path(&registry_path);
    let workspace_root = std::env::current_dir()
        .map_err(|error| ServerInitError::new("workspace 根目录获取", error.to_string()))?;

    let registry = ProviderRegistry::load_or_default(&registry_path)
        .map_err(|error| ServerInitError::new("provider 注册表加载", error.to_string()))?;
    let channel_registry = ChannelRegistry::load_or_default(&channel_registry_path)
        .map_err(|error| ServerInitError::new("channel 注册表加载", error.to_string()))?;

    let store = Arc::new(
        AiaStore::new(&aia_store_path)
            .map_err(|error| ServerInitError::new("数据库初始化", error.to_string()))?,
    );

    std::fs::create_dir_all(&sessions_dir)
        .map_err(|error| ServerInitError::new("sessions 目录创建", error.to_string()))?;

    let first_session_id = store
        .first_session_id_async()
        .await
        .map_err(|error| ServerInitError::new("session 首条记录加载", error.to_string()))?;
    if first_session_id.is_none() {
        let session_id = generate_session_id();
        let model_name = registry
            .active_provider()
            .and_then(|provider| provider.active_model.clone())
            .unwrap_or_default();
        let record = SessionRecord::new(
            session_id,
            aia_config::DEFAULT_SESSION_TITLE.to_string(),
            model_name,
        );
        store
            .create_session_async(record)
            .await
            .map_err(|error| ServerInitError::new("默认 session 创建", error.to_string()))?;
    }

    let selection = registry
        .active_provider()
        .cloned()
        .map(ProviderLaunchChoice::OpenAi)
        .unwrap_or(ProviderLaunchChoice::Bootstrap);

    let (identity, _model) = build_model_from_selection(selection, Some(store.clone()))
        .map_err(|error| ServerInitError::new("模型构建", error.to_string()))?;

    let (broadcast_tx, _) =
        tokio::sync::broadcast::channel(aia_config::DEFAULT_SERVER_EVENT_BUFFER);
    let provider_registry_snapshot = Arc::new(RwLock::new(registry.clone()));
    let provider_info_snapshot =
        Arc::new(RwLock::new(ProviderInfoSnapshot::from_identity(&identity)));
    let channel_registry_snapshot = Arc::new(RwLock::new(channel_registry));

    let session_manager = spawn_session_manager(SessionManagerConfig {
        sessions_dir,
        store: store.clone(),
        registry,
        store_path: registry_path,
        broadcast_tx: broadcast_tx.clone(),
        provider_registry_snapshot: provider_registry_snapshot.clone(),
        provider_info_snapshot: provider_info_snapshot.clone(),
        workspace_root,
        user_agent: build_server_user_agent(),
    });

    let state = Arc::new(AppState {
        session_manager,
        broadcast_tx,
        provider_registry_snapshot,
        provider_info_snapshot,
        channel_registry_path,
        channel_registry_snapshot,
        store,
    });

    sync_feishu_runtime(state.as_ref())
        .await
        .map_err(|error| ServerInitError::new("飞书通道启动", error))?;

    Ok(state)
}
