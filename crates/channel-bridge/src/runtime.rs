use std::{collections::HashMap, fmt, sync::Arc};

use agent_core::StreamEvent;
use agent_runtime::TurnLifecycle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use crate::{ChannelBindingStore, ChannelBridgeError, ChannelSessionService};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelTransport {
    Feishu,
    Weixin,
}

impl ChannelTransport {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Feishu => "feishu",
            Self::Weixin => "weixin",
        }
    }
}

impl fmt::Display for ChannelTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelProfile {
    pub id: String,
    pub name: String,
    pub transport: ChannelTransport,
    pub enabled: bool,
    #[serde(default)]
    pub config: Value,
}

impl ChannelProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        transport: ChannelTransport,
        config: Value,
    ) -> Self {
        Self { id: id.into(), name: name.into(), transport, enabled: true, config }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelTurnStatus {
    Waiting,
    Thinking,
    Working,
    Generating,
    Finishing,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelCurrentTurnSnapshot {
    pub turn_id: String,
    pub started_at_ms: u64,
    pub user_message: String,
    pub status: ChannelTurnStatus,
}

#[derive(Clone, Debug)]
pub enum ChannelRuntimeEvent {
    CurrentTurnStarted { session_id: String, current_turn: ChannelCurrentTurnSnapshot },
    Status { session_id: String, turn_id: String, status: ChannelTurnStatus },
    Stream { session_id: String, turn_id: String, event: StreamEvent },
    TurnCompleted { session_id: String, turn_id: String, turn: TurnLifecycle },
    Error { session_id: String, turn_id: Option<String>, message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SupportedChannelDefinition {
    pub transport: ChannelTransport,
    pub label: String,
    pub description: Option<String>,
    pub config_schema: Value,
}

#[async_trait::async_trait]
pub trait ChannelRuntimeHost: ChannelSessionService + ChannelBindingStore + Send + Sync {
    async fn submit_turn(&self, session_id: String, prompt: String) -> Result<String, String>;

    fn subscribe_runtime_events(&self) -> UnboundedReceiver<ChannelRuntimeEvent>;
}

pub trait ChannelRuntimeAdapter: Send + Sync {
    fn transport(&self) -> ChannelTransport;

    fn definition(&self) -> SupportedChannelDefinition;

    fn validate_config(&self, config: &Value) -> Result<(), ChannelBridgeError>;

    fn fingerprint(&self, profile: &ChannelProfile) -> Result<String, ChannelBridgeError>;

    fn spawn(&self, profile: ChannelProfile) -> Result<JoinHandle<()>, ChannelBridgeError>;
}

#[derive(Clone, Default)]
pub struct ChannelAdapterCatalog {
    adapters: Vec<Arc<dyn ChannelRuntimeAdapter>>,
}

impl ChannelAdapterCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, adapter: Arc<dyn ChannelRuntimeAdapter>) {
        self.adapters.retain(|existing| existing.transport() != adapter.transport());
        self.adapters.push(adapter);
    }

    pub fn definitions(&self) -> Vec<SupportedChannelDefinition> {
        self.adapters.iter().map(|adapter| adapter.definition()).collect()
    }

    pub fn adapter_for(
        &self,
        transport: &ChannelTransport,
    ) -> Option<Arc<dyn ChannelRuntimeAdapter>> {
        self.adapters.iter().find(|adapter| adapter.transport() == *transport).cloned()
    }
}

struct RunningChannelWorker {
    fingerprint: String,
    handle: JoinHandle<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeWorkerState {
    fingerprint: String,
    finished: bool,
}

struct DesiredWorker {
    adapter: Arc<dyn ChannelRuntimeAdapter>,
    profile: ChannelProfile,
    fingerprint: String,
}

pub struct ChannelRuntimeSupervisor {
    adapters: ChannelAdapterCatalog,
    workers: HashMap<String, RunningChannelWorker>,
}

impl ChannelRuntimeSupervisor {
    pub fn new(adapters: ChannelAdapterCatalog) -> Self {
        Self { adapters, workers: HashMap::new() }
    }

    pub fn sync(&mut self, profiles: Vec<ChannelProfile>) -> Result<(), ChannelBridgeError> {
        let mut desired = self.build_desired_workers(profiles)?;
        let existing = self
            .workers
            .iter()
            .map(|(profile_id, worker)| {
                (
                    profile_id.clone(),
                    RuntimeWorkerState {
                        fingerprint: worker.fingerprint.clone(),
                        finished: worker.handle.is_finished(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let (stop_ids, start_ids) = reconcile_runtime_workers(&existing, &desired);

        for profile_id in stop_ids {
            if let Some(worker) = self.workers.remove(&profile_id) {
                worker.handle.abort();
            }
        }

        for profile_id in start_ids {
            let Some(worker) = desired.remove(&profile_id) else {
                continue;
            };
            if let Some(existing_worker) = self.workers.remove(&profile_id) {
                existing_worker.handle.abort();
            }
            self.workers.insert(
                profile_id,
                RunningChannelWorker {
                    fingerprint: worker.fingerprint,
                    handle: worker.adapter.spawn(worker.profile)?,
                },
            );
        }

        Ok(())
    }

    fn build_desired_workers(
        &self,
        profiles: Vec<ChannelProfile>,
    ) -> Result<HashMap<String, DesiredWorker>, ChannelBridgeError> {
        let mut desired = HashMap::new();
        for profile in profiles {
            let Some(adapter) = self.adapter_for(&profile.transport) else {
                return Err(ChannelBridgeError::new(format!(
                    "missing channel runtime adapter for transport {:?}",
                    profile.transport
                )));
            };
            adapter.validate_config(&profile.config)?;
            let fingerprint = adapter.fingerprint(&profile)?;
            desired.insert(profile.id.clone(), DesiredWorker { adapter, profile, fingerprint });
        }
        Ok(desired)
    }

    fn adapter_for(&self, transport: &ChannelTransport) -> Option<Arc<dyn ChannelRuntimeAdapter>> {
        self.adapters.adapter_for(transport)
    }
}

impl Drop for ChannelRuntimeSupervisor {
    fn drop(&mut self) {
        for worker in self.workers.drain().map(|(_, worker)| worker) {
            worker.handle.abort();
        }
    }
}

fn reconcile_runtime_workers(
    existing: &HashMap<String, RuntimeWorkerState>,
    desired: &HashMap<String, DesiredWorker>,
) -> (Vec<String>, Vec<String>) {
    let stop_ids = existing
        .iter()
        .filter_map(|(profile_id, state)| match desired.get(profile_id) {
            Some(desired_worker)
                if desired_worker.fingerprint == state.fingerprint && !state.finished =>
            {
                None
            }
            _ => Some(profile_id.clone()),
        })
        .collect::<Vec<_>>();
    let start_ids = desired
        .iter()
        .filter_map(|(profile_id, desired_worker)| match existing.get(profile_id) {
            Some(state) if !state.finished && state.fingerprint == desired_worker.fingerprint => {
                None
            }
            _ => Some(profile_id.clone()),
        })
        .collect::<Vec<_>>();

    (stop_ids, start_ids)
}

#[cfg(test)]
#[path = "../tests/runtime/mod.rs"]
mod tests;
