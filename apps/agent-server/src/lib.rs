pub mod bootstrap;
pub mod session_manager;
pub mod state;

mod channel_host;
mod model;
mod routes;
mod runtime_worker;
mod self_chat;
mod server;
mod sse;

pub use bootstrap::{
    ServerBootstrapOptions, ServerInitError, bootstrap_state, bootstrap_state_with_options,
    build_server_user_agent,
};
pub use self_chat::{run_self_chat, self_chat_bootstrap_options};
pub use server::{ServerRunOptions, run_server, run_server_with_options};
pub use session_manager::{ProviderInfoSnapshot, RuntimeWorkerError, SessionManagerHandle};
pub use sse::{SsePayload, TurnStatus};
pub use state::{AppState, SharedState, Snapshot};
