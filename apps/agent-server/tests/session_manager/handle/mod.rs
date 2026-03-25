use tokio::sync::mpsc;

use super::super::SessionManagerHandle;

impl SessionManagerHandle {
    pub(crate) fn test_handle() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self {
            tx,
            workspace_root: std::path::PathBuf::new(),
        }
    }
}
