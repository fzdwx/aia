use std::sync::{Arc, Mutex};

use agent_core::{
    CoreError, LanguageModel, RuntimeToolContext, RuntimeToolContextStats, RuntimeToolHost,
    ToolExecutor,
};

use super::AgentRuntime;

pub(super) struct RuntimeToolContextAdapter {
    total_entries: usize,
    anchor_count: usize,
    entries_since_last_anchor: usize,
    last_input_tokens: Option<u32>,
    context_limit: Option<u32>,
    output_limit: Option<u32>,
    pressure_ratio: Option<f64>,
    pending_handoffs: Mutex<Vec<(String, String)>>,
    host_delegate: Option<Arc<dyn RuntimeToolHost>>,
}

impl RuntimeToolContextAdapter {
    pub(super) fn new<M, T>(runtime: &AgentRuntime<M, T>) -> Arc<Self>
    where
        M: LanguageModel,
        T: ToolExecutor,
    {
        let stats = runtime.context_stats();
        Arc::new(Self {
            total_entries: stats.total_entries,
            anchor_count: stats.anchor_count,
            entries_since_last_anchor: stats.entries_since_last_anchor,
            last_input_tokens: stats.last_input_tokens,
            context_limit: stats.context_limit,
            output_limit: stats.output_limit,
            pressure_ratio: stats.pressure_ratio,
            pending_handoffs: Mutex::new(Vec::new()),
            host_delegate: runtime.runtime_tool_host(),
        })
    }

    pub(super) fn drain_handoffs(&self) -> Vec<(String, String)> {
        let mut guard =
            self.pending_handoffs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *guard)
    }
}

impl RuntimeToolContext for RuntimeToolContextAdapter {
    fn context_stats(&self) -> RuntimeToolContextStats {
        RuntimeToolContextStats {
            total_entries: self.total_entries,
            anchor_count: self.anchor_count,
            entries_since_last_anchor: self.entries_since_last_anchor,
            last_input_tokens: self.last_input_tokens,
            context_limit: self.context_limit,
            output_limit: self.output_limit,
            pressure_ratio: self.pressure_ratio,
        }
    }

    fn record_handoff(&self, name: &str, summary: &str) -> Result<(), CoreError> {
        self.pending_handoffs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((name.to_string(), summary.to_string()));
        Ok(())
    }
}

impl RuntimeToolContextAdapter {
    pub(super) fn host_delegate(&self) -> Option<Arc<dyn RuntimeToolHost>> {
        self.host_delegate.clone()
    }
}
