use anyhow::ensure;
use async_trait::async_trait;
use gateway_core::{
    AttachmentRef, InboundEvent, MarkReadParams, OutboundReceipt, SendAttachmentParams, SendParams,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::broadcast;
use uuid::Uuid;

#[async_trait]
pub trait SignalClient: Send + Sync + 'static {
    async fn link_device(&self, account_id: &str) -> anyhow::Result<String>;
    async fn send_text(&self, params: SendParams) -> anyhow::Result<OutboundReceipt>;
    async fn send_attachment(
        &self,
        params: SendAttachmentParams,
    ) -> anyhow::Result<OutboundReceipt>;
    async fn mark_read(&self, _params: MarkReadParams) -> anyhow::Result<()>;
    fn subscribe(&self) -> broadcast::Receiver<InboundEvent>;
    fn receive_loop_live(&self) -> bool;
}

#[derive(Clone)]
pub struct MockSignalClient {
    account_id: String,
    tx: broadcast::Sender<InboundEvent>,
    live: Arc<AtomicBool>,
}

impl MockSignalClient {
    pub fn new(account_id: impl Into<String>) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            account_id: account_id.into(),
            tx,
            live: Arc::new(AtomicBool::new(true)),
        }
    }

    fn emit_text(&self, conversation_id: String, sender_id: String, text: String) {
        let _ = self.tx.send(InboundEvent::text_message(
            self.account_id.clone(),
            conversation_id,
            sender_id,
            text,
        ));
    }
}

#[async_trait]
impl SignalClient for MockSignalClient {
    async fn link_device(&self, account_id: &str) -> anyhow::Result<String> {
        ensure!(account_id == self.account_id, "account id mismatch");
        Ok(format!("sgd://link/{account_id}"))
    }

    async fn send_text(&self, params: SendParams) -> anyhow::Result<OutboundReceipt> {
        ensure!(params.account_id == self.account_id, "account id mismatch");
        let message_id = Uuid::new_v4().to_string();
        self.emit_text(
            params.conversation_id.clone(),
            "remote:mock".to_string(),
            format!("echo: {}", params.text),
        );
        Ok(OutboundReceipt {
            accepted: true,
            message_id,
        })
    }

    async fn send_attachment(
        &self,
        params: SendAttachmentParams,
    ) -> anyhow::Result<OutboundReceipt> {
        ensure!(params.account_id == self.account_id, "account id mismatch");
        let message_id = Uuid::new_v4().to_string();
        let attachment_names = params
            .attachments
            .iter()
            .map(|AttachmentRef { name, .. }| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        self.emit_text(
            params.conversation_id.clone(),
            "remote:mock".to_string(),
            format!("attachment echo: {attachment_names}"),
        );
        Ok(OutboundReceipt {
            accepted: true,
            message_id,
        })
    }

    async fn mark_read(&self, _params: MarkReadParams) -> anyhow::Result<()> {
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<InboundEvent> {
        self.tx.subscribe()
    }

    fn receive_loop_live(&self) -> bool {
        self.live.load(Ordering::Relaxed)
    }
}
