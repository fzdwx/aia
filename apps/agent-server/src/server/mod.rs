use crate::{bootstrap::ServerInitError, routes, state::SharedState};

#[derive(Clone, Debug)]
pub struct ServerRunOptions {
    bind_addr: Option<String>,
}

impl ServerRunOptions {
    pub fn with_bind_addr(mut self, bind_addr: impl Into<String>) -> Self {
        self.bind_addr = Some(bind_addr.into());
        self
    }

    fn bind_addr(&self) -> &str {
        self.bind_addr.as_deref().unwrap_or(aia_config::DEFAULT_SERVER_BIND_ADDR)
    }

    fn display_base_url(&self) -> String {
        let Some(bind_addr) = self.bind_addr.as_deref() else {
            return aia_config::DEFAULT_SERVER_BASE_URL.to_string();
        };
        format!("http://{}", display_host_for_bind_addr(bind_addr))
    }
}

impl Default for ServerRunOptions {
    fn default() -> Self {
        Self { bind_addr: None }
    }
}

pub async fn run_server(state: SharedState) -> Result<(), ServerInitError> {
    run_server_with_options(state, ServerRunOptions::default()).await
}

pub async fn run_server_with_options(
    state: SharedState,
    options: ServerRunOptions,
) -> Result<(), ServerInitError> {
    let app = routes::build_router(state);
    let bind_addr = options.bind_addr().to_string();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.map_err(|error| {
        ServerInitError::new("server listener 绑定", format!("{bind_addr}: {error}"))
    })?;
    println!("agent-server listening on {}", options.display_base_url());

    axum::serve(listener, app)
        .await
        .map_err(|error| ServerInitError::new("服务器启动", error.to_string()))?;

    Ok(())
}

fn display_host_for_bind_addr(bind_addr: &str) -> String {
    let Some((host, port)) = split_bind_addr(bind_addr) else {
        return bind_addr.to_string();
    };

    let display_host = match host {
        "0.0.0.0" | "::" | "[::]" => "localhost",
        value => value,
    };
    format!("{display_host}:{port}")
}

fn split_bind_addr(bind_addr: &str) -> Option<(&str, &str)> {
    if let Some(stripped) = bind_addr.strip_prefix('[') {
        let closing = stripped.find(']')?;
        let host_end = closing + 1;
        let host = &bind_addr[..=host_end];
        let port = bind_addr.get(host_end + 2..)?;
        return Some((host, port));
    }

    let separator = bind_addr.rfind(':')?;
    Some((&bind_addr[..separator], &bind_addr[separator + 1..]))
}

#[cfg(test)]
#[path = "../../tests/server/mod.rs"]
mod tests;
