use std::path::{Path, PathBuf};

pub const AIA_DIR_NAME: &str = ".aia";
pub const PROVIDERS_FILE_NAME: &str = "providers.json";
pub const SESSION_TAPE_FILE_NAME: &str = "session.jsonl";
pub const STORE_FILE_NAME: &str = "store.sqlite3";
pub const SESSIONS_DIR_NAME: &str = "sessions";

pub fn aia_dir_path() -> PathBuf {
    PathBuf::from(AIA_DIR_NAME)
}

pub fn default_registry_path() -> PathBuf {
    aia_dir_path().join(PROVIDERS_FILE_NAME)
}

pub fn default_session_tape_path() -> PathBuf {
    aia_dir_path().join(SESSION_TAPE_FILE_NAME)
}

pub fn default_store_path() -> PathBuf {
    aia_dir_path().join(STORE_FILE_NAME)
}

pub fn default_sessions_dir() -> PathBuf {
    aia_dir_path().join(SESSIONS_DIR_NAME)
}

pub fn sessions_dir_from_registry_path(registry_path: &Path) -> PathBuf {
    registry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SESSIONS_DIR_NAME)
}

pub fn store_path_from_registry_path(registry_path: &Path) -> PathBuf {
    registry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(STORE_FILE_NAME)
}
