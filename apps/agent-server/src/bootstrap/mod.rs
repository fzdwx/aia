use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use agent_core::RequestTimeoutConfig;
use agent_runtime::RuntimeHooks;
use agent_store::{
    AiaStore, SessionAutoRenamePolicy, SessionRecord, SessionTitleSource, generate_session_id,
};
use channel_bridge::ChannelProfileRegistry;
use provider_registry::ProviderRegistry;

use crate::{
    channel_host::{build_channel_adapter_catalog, build_channel_runtime, sync_channel_runtime},
    model::{ProviderLaunchChoice, model_identity_from_selection},
    session_manager::{ProviderInfoSnapshot, SessionManagerConfig, spawn_session_manager},
    state::AppState,
};

pub fn build_server_user_agent() -> String {
    aia_config::build_user_agent(aia_config::APP_NAME, env!("CARGO_PKG_VERSION"))
}

/// High-level bootstrap options for embedding `agent-server` as a reusable control plane.
///
/// This keeps callers on a stable configuration surface instead of constructing
/// `SessionManagerConfig` directly.
#[derive(Clone, Default)]
pub struct ServerBootstrapOptions {
    data_dir: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    user_agent: Option<String>,
    request_timeout: Option<RequestTimeoutConfig>,
    system_prompt: Option<String>,
    runtime_hooks: RuntimeHooks,
}

impl ServerBootstrapOptions {
    pub fn with_data_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(path.into());
        self
    }

    pub fn with_workspace_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace_root = Some(path.into());
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn with_request_timeout(mut self, request_timeout: RequestTimeoutConfig) -> Self {
        self.request_timeout = Some(request_timeout);
        self
    }

    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    pub fn with_runtime_hooks(mut self, runtime_hooks: RuntimeHooks) -> Self {
        self.runtime_hooks = runtime_hooks;
        self
    }
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
    bootstrap_state_with_options(ServerBootstrapOptions::default()).await
}

pub async fn bootstrap_state_with_options(
    options: ServerBootstrapOptions,
) -> Result<Arc<AppState>, ServerInitError> {
    ServerBootstrap::discover(options)?.bootstrap().await
}

struct ServerBootstrap {
    paths: BootstrapPaths,
    options: ServerBootstrapOptions,
}

struct BootstrapPaths {
    store_path: PathBuf,
    sessions_dir: PathBuf,
    workspace_root: PathBuf,
}

struct BootstrapResources {
    registry: ProviderRegistry,
    store: Arc<AiaStore>,
    channel_profile_registry: ChannelProfileRegistry,
}

struct BootstrapSnapshots {
    broadcast_tx: tokio::sync::broadcast::Sender<crate::sse::SsePayload>,
    provider_registry_snapshot: Arc<RwLock<ProviderRegistry>>,
    provider_info_snapshot: Arc<RwLock<ProviderInfoSnapshot>>,
    channel_profile_registry_snapshot: Arc<RwLock<ChannelProfileRegistry>>,
}

impl ServerBootstrap {
    fn discover(options: ServerBootstrapOptions) -> Result<Self, ServerInitError> {
        Ok(Self { paths: BootstrapPaths::discover(&options)?, options })
    }

    async fn bootstrap(self) -> Result<Arc<AppState>, ServerInitError> {
        let resources = self.load_resources().await?;
        self.prepare_sessions_dir()?;
        self.ensure_default_session(&resources).await?;
        let snapshots = self.build_snapshots(&resources);
        let state = self.assemble_state(resources, snapshots);
        self.start_channel_runtime(state).await
    }

    async fn load_resources(&self) -> Result<BootstrapResources, ServerInitError> {
        let store = Arc::new(
            AiaStore::new(&self.paths.store_path)
                .map_err(|error| ServerInitError::new("数据库初始化", error.to_string()))?,
        );
        let registry = store
            .load_provider_registry_async()
            .await
            .map_err(|error| ServerInitError::new("provider 注册表加载", error.to_string()))?;
        let channel_profile_registry = ChannelProfileRegistry::load_from_store(&store)
            .await
            .map_err(|error| ServerInitError::new("channel 档案注册表加载", error.to_string()))?;

        Ok(BootstrapResources { registry, store, channel_profile_registry })
    }

    fn prepare_sessions_dir(&self) -> Result<(), ServerInitError> {
        std::fs::create_dir_all(&self.paths.sessions_dir)
            .map_err(|error| ServerInitError::new("sessions 目录创建", error.to_string()))
    }

    async fn ensure_default_session(
        &self,
        resources: &BootstrapResources,
    ) -> Result<(), ServerInitError> {
        let first_session_id = resources
            .store
            .first_session_id_async()
            .await
            .map_err(|error| ServerInitError::new("session 首条记录加载", error.to_string()))?;
        if first_session_id.is_some() {
            return Ok(());
        }

        let session_id = generate_session_id();
        let model_name = resources
            .registry
            .first_provider()
            .and_then(|provider| provider.default_model_id().map(str::to_string))
            .unwrap_or_default();
        let record = SessionRecord::new_with_metadata(
            session_id,
            aia_config::DEFAULT_SESSION_TITLE.to_string(),
            model_name,
            SessionTitleSource::Default,
            SessionAutoRenamePolicy::Enabled,
        );
        resources
            .store
            .create_session_async(record)
            .await
            .map_err(|error| ServerInitError::new("默认 session 创建", error.to_string()))
    }

    fn build_snapshots(&self, resources: &BootstrapResources) -> BootstrapSnapshots {
        let selection = resources
            .registry
            .first_model_ref()
            .and_then(|model_ref| resources.registry.resolve_model(&model_ref).ok())
            .map(|spec| ProviderLaunchChoice::Resolved { spec, reasoning_effort: None })
            .unwrap_or(ProviderLaunchChoice::Bootstrap);
        let identity = model_identity_from_selection(&selection);

        let (broadcast_tx, _) =
            tokio::sync::broadcast::channel(aia_config::DEFAULT_SERVER_EVENT_BUFFER);
        let provider_registry_snapshot = Arc::new(RwLock::new(resources.registry.clone()));
        let provider_info_snapshot =
            Arc::new(RwLock::new(ProviderInfoSnapshot::from_identity(&identity)));
        let channel_profile_registry_snapshot =
            Arc::new(RwLock::new(resources.channel_profile_registry.clone()));

        BootstrapSnapshots {
            broadcast_tx,
            provider_registry_snapshot,
            provider_info_snapshot,
            channel_profile_registry_snapshot,
        }
    }

    fn assemble_state(
        &self,
        resources: BootstrapResources,
        snapshots: BootstrapSnapshots,
    ) -> Arc<AppState> {
        let session_manager = spawn_session_manager(SessionManagerConfig {
            sessions_dir: self.paths.sessions_dir.clone(),
            store: resources.store.clone(),
            registry: resources.registry,
            broadcast_tx: snapshots.broadcast_tx.clone(),
            provider_registry_snapshot: snapshots.provider_registry_snapshot.clone(),
            provider_info_snapshot: snapshots.provider_info_snapshot.clone(),
            workspace_root: self.paths.workspace_root.clone(),
            user_agent: self.options.user_agent.clone().unwrap_or_else(build_server_user_agent),
            request_timeout: self.options.request_timeout.clone().unwrap_or_else(|| {
                RequestTimeoutConfig {
                    read_timeout_ms: Some(aia_config::DEFAULT_SERVER_REQUEST_TIMEOUT_MS),
                }
            }),
            system_prompt: self.options.system_prompt.clone(),
            runtime_hooks: self.options.runtime_hooks.clone(),
            runtime_tool_host: std::sync::Arc::new(crate::session_manager::RuntimeToolHost {
                tx: tokio::sync::mpsc::channel(1).0,
            }),
        });
        let channel_adapter_catalog = Arc::new(build_channel_adapter_catalog(
            resources.store.clone(),
            session_manager.clone(),
            snapshots.broadcast_tx.clone(),
        ));
        let channel_runtime = Arc::new(tokio::sync::Mutex::new(build_channel_runtime(
            channel_adapter_catalog.as_ref().clone(),
        )));

        Arc::new(AppState::new(
            session_manager,
            snapshots.broadcast_tx,
            snapshots.provider_registry_snapshot,
            snapshots.provider_info_snapshot,
            snapshots.channel_profile_registry_snapshot,
            resources.store,
            channel_adapter_catalog,
            channel_runtime,
        ))
    }

    async fn start_channel_runtime(
        &self,
        state: Arc<AppState>,
    ) -> Result<Arc<AppState>, ServerInitError> {
        sync_channel_runtime(state.as_ref())
            .await
            .map_err(|error| ServerInitError::new("飞书通道启动", error))?;
        Ok(state)
    }
}

impl BootstrapPaths {
    fn discover(options: &ServerBootstrapOptions) -> Result<Self, ServerInitError> {
        let data_dir = options.data_dir.clone().unwrap_or_else(aia_config::aia_dir_path);
        let store_path = data_dir.join(aia_config::STORE_FILE_NAME);
        let sessions_dir = data_dir.join(aia_config::SESSIONS_DIR_NAME);
        let workspace_root = match options.workspace_root.clone() {
            Some(path) => path,
            None => std::env::current_dir()
                .map_err(|error| ServerInitError::new("workspace 根目录获取", error.to_string()))?,
        };

        Ok(Self { store_path, sessions_dir, workspace_root })
    }
}

#[cfg(test)]
#[path = "../../tests/bootstrap/mod.rs"]
mod tests;
