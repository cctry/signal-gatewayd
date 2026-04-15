use gateway_core::GatewayConfig;
use gateway_http::{AppState, router};
use gateway_signal::MockSignalClient;
use gateway_store::GatewayStore;
use std::sync::Arc;
use std::time::Duration;
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

    let backend = std::env::var("SIGNAL_GATEWAY_BACKEND").unwrap_or_else(|_| "mock".to_string());
    #[cfg(feature = "presage-backend")]
    let client: Arc<dyn gateway_signal::SignalClient> = match backend.as_str() {
        "presage" => {
            let presage_store = std::env::var("SIGNAL_PRESAGE_DB_PATH")
                .unwrap_or_else(|_| format!("{}.presage.sqlite", cfg.store_path));
            let device_name = std::env::var("SIGNAL_PRESAGE_DEVICE_NAME")
                .unwrap_or_else(|_| "signal-gatewayd".to_string());
            Arc::new(
                gateway_signal::PresageSignalClient::open(
                    cfg.account_id.clone(),
                    presage_store,
                    device_name,
                )
                .await?,
            )
        }
        _ => Arc::new(MockSignalClient::new(cfg.account_id.clone())),
    };

    #[cfg(not(feature = "presage-backend"))]
    let client: Arc<dyn gateway_signal::SignalClient> = match backend.as_str() {
        "presage" => {
            anyhow::bail!("signal-gatewayd was built without the `presage-backend` feature")
        }
        _ => Arc::new(MockSignalClient::new(cfg.account_id.clone())),
    };

    let sync_store = store.clone();
    let sync_cfg = cfg.clone();
    let sync_client = client.clone();
    tokio::spawn(async move {
        loop {
            if let Err(err) = sync_store.set_linked(&sync_cfg, sync_client.linked(), None) {
                tracing::warn!(error = %err, "failed to sync linked state");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    let app = router(AppState::new(cfg.clone(), store, client));

    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    info!(backend = %backend, "signal-gatewayd listening on {}", cfg.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
