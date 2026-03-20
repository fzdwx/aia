use crate::{bootstrap::ServerInitError, routes, state::SharedState};

pub async fn run_server(state: SharedState) -> Result<(), ServerInitError> {
    let app = routes::build_router(state);
    let listener = tokio::net::TcpListener::bind(aia_config::DEFAULT_SERVER_BIND_ADDR)
        .await
        .map_err(|error| ServerInitError::new("端口 3434 绑定", error.to_string()))?;
    println!("agent-server listening on {}", aia_config::DEFAULT_SERVER_BASE_URL);

    axum::serve(listener, app)
        .await
        .map_err(|error| ServerInitError::new("服务器启动", error.to_string()))?;

    Ok(())
}
