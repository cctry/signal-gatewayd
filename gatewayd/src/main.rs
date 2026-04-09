use gateway_core::GatewayConfig;
use gateway_http::{AppState, router};
use gateway_signal::MockSignalClient;
use gateway_store::GatewayStore;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .compact()
        .init();

    let cfg = GatewayConfig::default();
    let store = GatewayStore::open(&cfg.store_path)?;
    store.ensure_account(&cfg)?;

    let client: Arc<dyn gateway_signal::SignalClient> =
        Arc::new(MockSignalClient::new(cfg.account_id.clone()));
    let app = router(AppState::new(cfg.clone(), store, client));

    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    info!("signal-gatewayd listening on {}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
