use gateway_core::{AdminStatus, GatewayConfig, LinkDeviceResponse};
use gateway_signal::SignalClient;
use gateway_store::GatewayStore;
use std::sync::Arc;

#[derive(Clone)]
pub struct AdminService {
    cfg: GatewayConfig,
    store: GatewayStore,
    client: Arc<dyn SignalClient>,
}

impl AdminService {
    pub fn new(cfg: GatewayConfig, store: GatewayStore, client: Arc<dyn SignalClient>) -> Self {
        Self { cfg, store, client }
    }

    pub async fn link_device(&self) -> anyhow::Result<LinkDeviceResponse> {
        let _ = self.client.link_device(&self.cfg.account_id).await?;
        self.store.mark_linked(&self.cfg)
    }

    pub fn status(&self) -> anyhow::Result<AdminStatus> {
        self.store
            .get_admin_status(&self.cfg, self.client.receive_loop_live())
    }
}
