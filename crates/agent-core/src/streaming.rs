use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{CoreError, QuestionRequest, QuestionResult};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug)]
pub struct ToolOutputDelta {
    pub stream: ToolOutputStream,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct AbortSignal(Arc<AtomicBool>);

impl AbortSignal {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn abort(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_aborted(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeToolContextStats {
    pub total_entries: usize,
    pub anchor_count: usize,
    pub entries_since_last_anchor: usize,
    pub last_input_tokens: Option<u32>,
    pub context_limit: Option<u32>,
    pub output_limit: Option<u32>,
    pub pressure_ratio: Option<f64>,
}

#[async_trait]
pub trait RuntimeToolHost: Send + Sync {
    async fn ask_question(
        &self,
        _session_id: &str,
        _request: QuestionRequest,
    ) -> Result<QuestionResult, CoreError> {
        Err(CoreError::new("question runtime capability unavailable"))
    }
}

pub trait RuntimeToolContext: Send + Sync {
    fn context_stats(&self) -> RuntimeToolContextStats;
    fn record_handoff(&self, name: &str, summary: &str) -> Result<(), CoreError>;
}

pub struct ToolExecutionContext {
    pub run_id: String,
    pub session_id: Option<String>,
    pub workspace_root: Option<PathBuf>,
    pub abort: AbortSignal,
    pub runtime: Option<Arc<dyn RuntimeToolContext>>,
    pub runtime_host: Option<Arc<dyn RuntimeToolHost>>,
}

impl ToolExecutionContext {
    pub fn resolve_path(&self, raw: &str) -> PathBuf {
        let path = Path::new(raw);
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(root) = &self.workspace_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    ThinkingDelta {
        text: String,
    },
    TextDelta {
        text: String,
    },
    ToolCallDetected {
        invocation_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        detected_at_ms: u64,
    },
    ToolCallStarted {
        invocation_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        started_at_ms: u64,
    },
    ToolOutputDelta {
        invocation_id: String,
        stream: ToolOutputStream,
        text: String,
    },
    ToolCallCompleted {
        invocation_id: String,
        tool_name: String,
        content: String,
        details: Option<serde_json::Value>,
        failed: bool,
        finished_at_ms: u64,
    },
    Log {
        text: String,
    },
    Done,
}
