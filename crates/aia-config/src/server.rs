pub const DEFAULT_SESSION_TITLE: &str = "New session";
pub const DEFAULT_SERVER_BIND_ADDR: &str = "0.0.0.0:3434";
pub const DEFAULT_SERVER_BASE_URL: &str = "http://localhost:3434";
pub const DEFAULT_SERVER_EVENT_BUFFER: usize = 512;
pub const DEFAULT_SERVER_REQUEST_TIMEOUT_MS: u64 = 300_000;

pub fn build_user_agent(app_name: &str, version: &str) -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    format!("{app_name}-{os}-{arch}/{version}")
}
