use std::{collections::HashMap, sync::Arc};

use agent_core::StreamEvent;
use agent_runtime::TurnLifecycle;
use channel_registry::{ChannelProfile, ChannelTransport};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use crate::{ChannelBindingStore, ChannelBridgeError, ChannelSessionService};

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

#[async_trait::async_trait]
pub trait ChannelRuntimeHost: ChannelSessionService + ChannelBindingStore + Send + Sync {
    async fn submit_turn(&self, session_id: String, prompt: String) -> Result<String, String>;

    fn subscribe_runtime_events(&self) -> UnboundedReceiver<ChannelRuntimeEvent>;
}

pub trait ChannelRuntimeAdapter: Send + Sync {
    fn transport(&self) -> ChannelTransport;

    fn fingerprint(&self, profile: &ChannelProfile) -> String;

    fn spawn(&self, profile: ChannelProfile) -> JoinHandle<()>;
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
    adapters: Vec<Arc<dyn ChannelRuntimeAdapter>>,
    workers: HashMap<String, RunningChannelWorker>,
}

impl ChannelRuntimeSupervisor {
    pub fn new(adapters: Vec<Arc<dyn ChannelRuntimeAdapter>>) -> Self {
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
                    handle: worker.adapter.spawn(worker.profile),
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
            let fingerprint = adapter.fingerprint(&profile);
            desired.insert(profile.id.clone(), DesiredWorker { adapter, profile, fingerprint });
        }
        Ok(desired)
    }

    fn adapter_for(&self, transport: &ChannelTransport) -> Option<Arc<dyn ChannelRuntimeAdapter>> {
        self.adapters.iter().find(|adapter| adapter.transport() == *transport).cloned()
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
mod tests {
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

        fn fingerprint(&self, profile: &ChannelProfile) -> String {
            format!("{}:{}:{}", profile.id, profile.enabled, profile.config.base_url)
        }

        fn spawn(&self, profile: ChannelProfile) -> JoinHandle<()> {
            self.spawned
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(profile.id.clone());
            tokio::spawn(async {})
        }
    }

    fn sample_profile(id: &str) -> ChannelProfile {
        ChannelProfile::new_feishu(id, "默认飞书", "app", "secret")
    }

    #[test]
    fn sync_errors_when_transport_has_no_adapter() {
        let mut supervisor = ChannelRuntimeSupervisor::new(vec![]);

        let error = supervisor
            .sync(vec![sample_profile("default")])
            .expect_err("missing adapter should fail");

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
        let mut supervisor = ChannelRuntimeSupervisor::new(vec![adapter.clone()]);
        let profile = sample_profile("default");

        supervisor.sync(vec![profile.clone()]).expect("first sync should succeed");
        supervisor.sync(vec![profile]).expect("second sync should succeed");
        tokio::task::yield_now().await;

        let spawned =
            adapter.spawned.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        assert_eq!(spawned, vec!["default"]);
    }
}
