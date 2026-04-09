use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Text,
    Attachment,
    Reaction,
    Receipt,
    Typing,
    GroupMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentRef {
    pub id: String,
    pub name: String,
    pub content_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupRef {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboundEvent {
    pub event_id: String,
    pub account_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub sender_profile: Option<String>,
    pub timestamp_server: DateTime<Utc>,
    pub timestamp_received_local: DateTime<Utc>,
    pub kind: EventKind,
    pub text: Option<String>,
    pub attachments: Vec<AttachmentRef>,
    pub group: Option<GroupRef>,
}

impl InboundEvent {
    pub fn text_message(
        account_id: impl Into<String>,
        conversation_id: impl Into<String>,
        sender_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            event_id: Uuid::new_v4().to_string(),
            account_id: account_id.into(),
            conversation_id: conversation_id.into(),
            sender_id: sender_id.into(),
            sender_profile: None,
            timestamp_server: now,
            timestamp_received_local: now,
            kind: EventKind::Text,
            text: Some(text.into()),
            attachments: Vec::new(),
            group: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendParams {
    pub account_id: String,
    pub conversation_id: String,
    pub text: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendAttachmentParams {
    pub account_id: String,
    pub conversation_id: String,
    pub text: Option<String>,
    pub attachments: Vec<AttachmentRef>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarkReadParams {
    pub account_id: String,
    pub conversation_id: String,
    pub event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundReceipt {
    pub accepted: bool,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayHealth {
    pub ok: bool,
    pub account_id: String,
    pub linked: bool,
    pub receive_loop_live: bool,
    pub last_successful_sync: Option<DateTime<Utc>>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminStatus {
    pub account_id: String,
    pub linked: bool,
    pub storage_path: String,
    pub receive_loop_live: bool,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkDeviceResponse {
    pub account_id: String,
    pub linked: bool,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayConfig {
    pub account_id: String,
    pub bind: SocketAddr,
    pub store_path: String,
    pub version: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            account_id: "default".to_string(),
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000),
            store_path: "signal-gatewayd.db".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("account is not linked")]
    AccountNotLinked,
    #[error("internal error: {0}")]
    Internal(String),
}
