pub mod auth;
pub mod cli_sender;
pub mod protocol;
pub mod routes;

use std::net::SocketAddr;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::bot::AppState;
use crate::config::schema::ApiConfig;
use crate::logger::Logger;

/// Shared state for every API route handler.
#[derive(Clone)]
pub struct ApiState {
    pub app_state: Arc<AppState>,
}

/// Start the local HTTP API that devm8-client talks to. Spawned as one more
/// task inside `start_polling`, alongside the Slack/Teams tasks — gated by
/// `config.api` being present (opt-in, same pattern as Slack/Teams).
///
/// TLS is optional: plain HTTP is safe over a Tailscale tailnet (transport is
/// already WireGuard-encrypted node-to-node). Setting `tls_cert_path`/
/// `tls_key_path` (e.g. from `tailscale cert <magicdns-name>`) switches to a
/// real Let's-Encrypt-backed HTTPS listener — useful if the API is ever
/// reachable somewhere Tailscale's own encryption isn't the only hop.
pub async fn run_api_server(
    ct: CancellationToken,
    app_state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    config: &ApiConfig,
) -> anyhow::Result<()> {
    let state = ApiState { app_state };
    let app = routes::build_router(state);

    let bind_addr = config.bind_addr.as_deref().unwrap_or("0.0.0.0");
    let addr = format!("{bind_addr}:{}", config.port);

    match (&config.tls_cert_path, &config.tls_key_path) {
        (Some(cert), Some(key)) => {
            let tls_config = RustlsConfig::from_pem_file(cert, key).await?;
            let socket_addr: SocketAddr = addr.parse()?;
            logger.info("api server listening (tls)", Some(&json!({ "addr": addr })));
            let handle = axum_server::Handle::new();
            let shutdown_handle = handle.clone();
            tokio::spawn(async move {
                ct.cancelled().await;
                shutdown_handle.graceful_shutdown(None);
            });
            axum_server::bind_rustls(socket_addr, tls_config)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;
        }
        _ => {
            let listener = TcpListener::bind(&addr).await?;
            logger.info("api server listening", Some(&json!({ "addr": addr })));
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { ct.cancelled().await })
                .await?;
        }
    }

    Ok(())
}
