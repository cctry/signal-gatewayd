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
        let uri = self.client.link_device(&self.cfg.account_id).await?;
        let linked = self.client.linked();
        if linked {
            self.store.set_linked(&self.cfg, true, Some(&uri))?;
        }
        Ok(LinkDeviceResponse {
            account_id: self.cfg.account_id.clone(),
            linked,
            uri,
        })
    }

    pub fn status(&self) -> anyhow::Result<AdminStatus> {
        self.store
            .get_admin_status(&self.cfg, self.client.receive_loop_live())
    }
}
