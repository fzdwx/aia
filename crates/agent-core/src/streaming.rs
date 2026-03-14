use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};

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
pub struct ToolExecutionContext {
    pub run_id: String,
    pub workspace_root: Option<PathBuf>,
    pub abort: AbortSignal,
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
    ThinkingDelta { text: String },
    TextDelta { text: String },
    ToolCallStarted { invocation_id: String, tool_name: String, arguments: serde_json::Value },
    ToolOutputDelta { invocation_id: String, stream: ToolOutputStream, text: String },
    ToolCallCompleted {
        invocation_id: String,
        tool_name: String,
        content: String,
        details: Option<serde_json::Value>,
        failed: bool,
    },
    Log { text: String },
    Done,
}
