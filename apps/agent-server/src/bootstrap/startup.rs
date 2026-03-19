use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use agent_store::{AiaStore, SessionRecord, generate_session_id};
use channel_bridge::ChannelProfileRegistry;
use provider_registry::ProviderRegistry;

use crate::{
    channel_host::{build_channel_adapter_catalog, build_channel_runtime, sync_channel_runtime},
    model::{ProviderLaunchChoice, model_identity_from_selection},
    session_manager::{ProviderInfoSnapshot, SessionManagerConfig, spawn_session_manager},
    state::AppState,
};

use super::{ServerInitError, build_server_user_agent};

pub(super) struct ServerBootstrap {
    paths: BootstrapPaths,
}

struct BootstrapPaths {
    registry_path: PathBuf,
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
    pub(super) fn discover() -> Result<Self, ServerInitError> {
        Ok(Self { paths: BootstrapPaths::discover()? })
    }

    pub(super) async fn bootstrap(self) -> Result<Arc<AppState>, ServerInitError> {
        let resources = self.load_resources().await?;
        self.prepare_sessions_dir()?;
        self.ensure_default_session(&resources).await?;
        let snapshots = self.build_snapshots(&resources)?;
        let state = self.assemble_state(resources, snapshots);
        self.start_channel_runtime(state).await
    }

    async fn load_resources(&self) -> Result<BootstrapResources, ServerInitError> {
        let registry = ProviderRegistry::load_or_default(&self.paths.registry_path)
            .map_err(|error| ServerInitError::new("provider 注册表加载", error.to_string()))?;
        let store = Arc::new(
            AiaStore::new(&self.paths.store_path)
                .map_err(|error| ServerInitError::new("数据库初始化", error.to_string()))?,
        );
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
            .active_provider()
            .and_then(|provider| provider.active_model.clone())
            .unwrap_or_default();
        let record = SessionRecord::new(
            session_id,
            aia_config::DEFAULT_SESSION_TITLE.to_string(),
            model_name,
        );
        resources
            .store
            .create_session_async(record)
            .await
            .map_err(|error| ServerInitError::new("默认 session 创建", error.to_string()))
    }

    fn build_snapshots(
        &self,
        resources: &BootstrapResources,
    ) -> Result<BootstrapSnapshots, ServerInitError> {
        let selection = resources
            .registry
            .active_provider()
            .cloned()
            .map(ProviderLaunchChoice::OpenAi)
            .unwrap_or(ProviderLaunchChoice::Bootstrap);
        let identity = model_identity_from_selection(&selection);

        let (broadcast_tx, _) =
            tokio::sync::broadcast::channel(aia_config::DEFAULT_SERVER_EVENT_BUFFER);
        let provider_registry_snapshot = Arc::new(RwLock::new(resources.registry.clone()));
        let provider_info_snapshot =
            Arc::new(RwLock::new(ProviderInfoSnapshot::from_identity(&identity)));
        let channel_profile_registry_snapshot =
            Arc::new(RwLock::new(resources.channel_profile_registry.clone()));

        Ok(BootstrapSnapshots {
            broadcast_tx,
            provider_registry_snapshot,
            provider_info_snapshot,
            channel_profile_registry_snapshot,
        })
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
            provider_registry_path: self.paths.registry_path.clone(),
            broadcast_tx: snapshots.broadcast_tx.clone(),
            provider_registry_snapshot: snapshots.provider_registry_snapshot.clone(),
            provider_info_snapshot: snapshots.provider_info_snapshot.clone(),
            workspace_root: self.paths.workspace_root.clone(),
            user_agent: build_server_user_agent(),
        });
        let channel_adapter_catalog = Arc::new(build_channel_adapter_catalog(
            resources.store.clone(),
            session_manager.clone(),
            snapshots.broadcast_tx.clone(),
        ));
        let channel_runtime = Arc::new(tokio::sync::Mutex::new(build_channel_runtime(
            channel_adapter_catalog.as_ref().clone(),
        )));

        Arc::new(AppState {
            session_manager,
            broadcast_tx: snapshots.broadcast_tx,
            provider_registry_snapshot: snapshots.provider_registry_snapshot,
            provider_info_snapshot: snapshots.provider_info_snapshot,
            channel_profile_registry_snapshot: snapshots.channel_profile_registry_snapshot,
            channel_mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
            store: resources.store,
            channel_adapter_catalog,
            channel_runtime,
        })
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
    fn discover() -> Result<Self, ServerInitError> {
        let registry_path = provider_registry::default_registry_path();
        let store_path = aia_config::store_path_from_registry_path(&registry_path);
        let sessions_dir = aia_config::sessions_dir_from_registry_path(&registry_path);
        let workspace_root = std::env::current_dir()
            .map_err(|error| ServerInitError::new("workspace 根目录获取", error.to_string()))?;

        Ok(Self { registry_path, store_path, sessions_dir, workspace_root })
    }
}
