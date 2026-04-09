use anyhow::ensure;
use async_trait::async_trait;
#[cfg(feature = "presage-backend")]
use gateway_core::{
    AttachmentRef, EventKind, GroupRef, InboundEvent, MarkReadParams, OutboundReceipt,
    SendAttachmentParams, SendParams,
};
#[cfg(not(feature = "presage-backend"))]
use gateway_core::{
    AttachmentRef, InboundEvent, MarkReadParams, OutboundReceipt, SendAttachmentParams, SendParams,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::broadcast;
#[cfg(feature = "presage-backend")]
use tracing::{error, info, warn};
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
    fn linked(&self) -> bool;
}

#[derive(Clone)]
pub struct MockSignalClient {
    account_id: String,
    tx: broadcast::Sender<InboundEvent>,
    live: Arc<AtomicBool>,
    linked: Arc<AtomicBool>,
}

impl MockSignalClient {
    pub fn new(account_id: impl Into<String>) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            account_id: account_id.into(),
            tx,
            live: Arc::new(AtomicBool::new(true)),
            linked: Arc::new(AtomicBool::new(false)),
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
        self.linked.store(true, Ordering::Relaxed);
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

    fn linked(&self) -> bool {
        self.linked.load(Ordering::Relaxed)
    }
}

#[cfg(feature = "presage-backend")]
mod presage_backend {
    use super::*;
    use anyhow::{Context, anyhow};
    use futures::{StreamExt, channel::oneshot};
    use mime_guess::mime::APPLICATION_OCTET_STREAM;
    use presage::{
        Manager,
        libsignal_service::{
            configuration::SignalServers,
            content::{ContentBody, DataMessage},
            prelude::Content,
            protocol::ServiceId,
        },
        manager::{Linking, Registered},
        model::messages::Received,
        model::identity::OnNewIdentity,
        store::{ContentExt, Thread},
    };
    use presage_store_sqlite::SqliteStore;
    use url::Url;

    #[derive(Clone)]
    pub struct PresageSignalClient {
        account_id: String,
        device_name: String,
        store_path: String,
        tx: broadcast::Sender<InboundEvent>,
        live: Arc<AtomicBool>,
        linked: Arc<AtomicBool>,
    }

    impl PresageSignalClient {
        pub async fn open(
            account_id: impl Into<String>,
            store_path: impl Into<String>,
            device_name: impl Into<String>,
        ) -> anyhow::Result<Self> {
            let (tx, _) = broadcast::channel(512);
            let client = Self {
                account_id: account_id.into(),
                device_name: device_name.into(),
                store_path: store_path.into(),
                tx,
                live: Arc::new(AtomicBool::new(false)),
                linked: Arc::new(AtomicBool::new(false)),
            };

            if let Some(manager) = client.load_registered_manager().await? {
                info!("loaded existing presage registration");
                client.linked.store(true, Ordering::Relaxed);
                client.live.store(true, Ordering::Relaxed);
                client.spawn_receive_loop(manager);
            }

            Ok(client)
        }

        async fn open_store(&self) -> anyhow::Result<SqliteStore> {
            SqliteStore::open(&self.store_path, OnNewIdentity::Trust)
                .await
                .with_context(|| {
                    format!("failed to open presage sqlite store at {}", self.store_path)
                })
        }

        async fn load_registered_manager(&self) -> anyhow::Result<Option<Manager<SqliteStore, Registered>>> {
            let store = self.open_store().await?;
            match Manager::load_registered(store).await {
                Ok(manager) => Ok(Some(manager)),
                Err(_) => Ok(None),
            }
        }

        fn spawn_receive_loop(&self, mut manager: Manager<SqliteStore, Registered>) {
            let tx = self.tx.clone();
            let live = self.live.clone();
            let linked = self.linked.clone();
            let account_id = self.account_id.clone();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build presage receive runtime");
                let local = tokio::task::LocalSet::new();
                local.block_on(&runtime, async move {
                    live.store(true, Ordering::Relaxed);
                    linked.store(true, Ordering::Relaxed);

                    let receive = manager.receive_messages().await;
                    let mut stream = match receive {
                        Ok(stream) => stream,
                        Err(err) => {
                            live.store(false, Ordering::Relaxed);
                            error!(error = %err, "failed to start presage receive loop");
                            return;
                        }
                    };

                    while let Some(received) = stream.next().await {
                        match received {
                            Received::QueueEmpty => info!("presage receive queue drained"),
                            Received::Contacts => info!("presage contact sync completed"),
                            Received::Content(content) => {
                                if let Some(event) = normalize_content(&account_id, content.as_ref())
                                {
                                    let _ = tx.send(event);
                                }
                            }
                        }
                    }

                    live.store(false, Ordering::Relaxed);
                    warn!("presage receive loop ended");
                });
            });
        }

        fn parse_service_id(&self, conversation_id: &str) -> anyhow::Result<ServiceId> {
            ServiceId::parse_from_service_id_string(conversation_id)
                .ok_or_else(|| anyhow!("invalid Signal service id: {conversation_id}"))
        }
    }

    fn attachment_refs_from_message(message: &DataMessage) -> Vec<AttachmentRef> {
        message
            .attachments
            .iter()
            .enumerate()
            .map(|(idx, attachment)| AttachmentRef {
                id: idx.to_string(),
                name: attachment
                    .file_name
                    .clone()
                    .unwrap_or_else(|| format!("attachment-{idx}")),
                content_type: attachment
                    .content_type
                    .clone()
                    .unwrap_or_else(|| APPLICATION_OCTET_STREAM.to_string()),
                size_bytes: attachment.size.unwrap_or_default() as u64,
            })
            .collect()
    }

    fn normalize_content(account_id: &str, content: &Content) -> Option<InboundEvent> {
        let thread = Thread::try_from(content).ok()?;
        let conversation_id = match thread {
            Thread::Contact(service_id) => service_id.service_id_string(),
            Thread::Group(master_key) => hex::encode(master_key),
        };
        let sender_id = content.metadata.sender.service_id_string();
        let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
            i64::try_from(content.timestamp()).ok()?,
        )?;

        let (kind, text, attachments, group) = match &content.body {
            ContentBody::DataMessage(message) => {
                let kind = if message.group_v2.is_some() {
                    EventKind::GroupMessage
                } else if !message.attachments.is_empty() {
                    EventKind::Attachment
                } else {
                    EventKind::Text
                };
                let group = message.group_v2.as_ref().and_then(|group| {
                    group.master_key.as_ref().map(|key| GroupRef {
                        id: hex::encode(key),
                        title: None,
                    })
                });
                (
                    kind,
                    message.body.clone(),
                    attachment_refs_from_message(message),
                    group,
                )
            }
            ContentBody::TypingMessage(_) => (EventKind::Typing, None, Vec::new(), None),
            ContentBody::ReceiptMessage(_) => (EventKind::Receipt, None, Vec::new(), None),
            ContentBody::SynchronizeMessage(sync) => {
                let sent_message = sync.sent.as_ref().and_then(|sent| sent.message.as_ref());
                let text = sent_message.and_then(|msg| msg.body.clone());
                let attachments = sent_message
                    .map(attachment_refs_from_message)
                    .unwrap_or_default();
                let group = sent_message.and_then(|msg| {
                    msg.group_v2.as_ref().and_then(|group| {
                        group.master_key.as_ref().map(|key| GroupRef {
                            id: hex::encode(key),
                            title: None,
                        })
                    })
                });
                (EventKind::Text, text, attachments, group)
            }
            _ => return None,
        };

        Some(InboundEvent {
            event_id: content
                .metadata
                .server_guid
                .clone()
                .map(|uuid| uuid.to_string())
                .unwrap_or_else(|| format!("{}:{}", sender_id, content.timestamp())),
            account_id: account_id.to_string(),
            conversation_id,
            sender_id,
            sender_profile: None,
            timestamp_server: timestamp,
            timestamp_received_local: chrono::Utc::now(),
            kind,
            text,
            attachments,
            group,
        })
    }

    #[async_trait]
    impl SignalClient for PresageSignalClient {
        async fn link_device(&self, account_id: &str) -> anyhow::Result<String> {
            ensure!(account_id == self.account_id, "account id mismatch");
            let store = self.open_store().await?;
            let device_name = self.device_name.clone();
            let tx = self.tx.clone();
            let live = self.live.clone();
            let linked = self.linked.clone();
            let account_id = self.account_id.clone();
            let store_path = self.store_path.clone();
            let (provisioning_tx, provisioning_rx) = tokio::sync::oneshot::channel();

            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build presage link runtime");
                let local = tokio::task::LocalSet::new();
                local.block_on(&runtime, async move {
                    let (url_tx, url_rx): (oneshot::Sender<Url>, oneshot::Receiver<Url>) =
                        oneshot::channel();
                    let forward_url = async move {
                        match url_rx.await {
                            Ok(url) => {
                                let _ = provisioning_tx.send(url.to_string());
                            }
                            Err(_) => {
                                let _ = provisioning_tx
                                    .send("failed to receive provisioning URL".to_string());
                            }
                        }
                    };

                    let link = Manager::<SqliteStore, Linking>::link_secondary_device(
                        store,
                        SignalServers::Production,
                        device_name,
                        url_tx,
                    );

                    let (link_result, _) = futures::future::join(link, forward_url).await;
                    match link_result {
                        Ok(manager) => {
                            linked.store(true, Ordering::Relaxed);
                            live.store(true, Ordering::Relaxed);
                            let client = PresageSignalClient {
                                account_id,
                                device_name: String::new(),
                                store_path,
                                tx,
                                live,
                                linked,
                            };
                            client.spawn_receive_loop(manager);
                        }
                        Err(err) => error!(error = %err, "presage link flow failed"),
                    }
                });
            });

            let url = provisioning_rx
                .await
                .map_err(|_| anyhow!("failed to receive provisioning URL from presage link flow"))?;
            if url.starts_with("failed to receive provisioning URL") {
                return Err(anyhow!(url));
            }
            Ok(url)
        }

        async fn send_text(&self, params: SendParams) -> anyhow::Result<OutboundReceipt> {
            ensure!(params.account_id == self.account_id, "account id mismatch");
            let _ = self.parse_service_id(&params.conversation_id)?;
            Err(anyhow!(
                "presage backend send path is not wired yet; it needs a dedicated local worker because upstream presage send futures are not Send"
            ))
        }

        async fn send_attachment(
            &self,
            params: SendAttachmentParams,
        ) -> anyhow::Result<OutboundReceipt> {
            ensure!(params.account_id == self.account_id, "account id mismatch");
            let _ = self.parse_service_id(&params.conversation_id)?;
            let _ = params.attachments;
            Err(anyhow!(
                "presage backend attachment send path is not wired yet; it needs a dedicated local worker because upstream presage send futures are not Send"
            ))
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

        fn linked(&self) -> bool {
            self.linked.load(Ordering::Relaxed)
        }
    }
}

#[cfg(feature = "presage-backend")]
pub use presage_backend::PresageSignalClient;
