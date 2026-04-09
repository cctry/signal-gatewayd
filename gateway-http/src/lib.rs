use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use chrono::Utc;
use gateway_admin::AdminService;
use gateway_core::{
    AttachmentRef, GatewayConfig, JsonRpcRequest, JsonRpcResponse, MarkReadParams,
    SendAttachmentParams, SendParams,
};
use gateway_signal::SignalClient;
use gateway_store::GatewayStore;
use serde::Deserialize;
use serde_json::json;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    cfg: GatewayConfig,
    store: GatewayStore,
    client: Arc<dyn SignalClient>,
    admin: AdminService,
}

impl AppState {
    pub fn new(cfg: GatewayConfig, store: GatewayStore, client: Arc<dyn SignalClient>) -> Self {
        let admin = AdminService::new(cfg.clone(), store.clone(), client.clone());
        Self {
            cfg,
            store,
            client,
            admin,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    account: Option<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/events", get(events))
        .route("/api/v1/rpc", post(rpc))
        .route("/admin/status", get(admin_status))
        .route("/admin/link-device", post(link_device))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match state
        .store
        .get_health(&state.cfg, state.client.receive_loop_live())
    {
        Ok(health) => Json(health).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match state
        .store
        .get_health(&state.cfg, state.client.receive_loop_live())
    {
        Ok(health) if health.linked && health.receive_loop_live => StatusCode::OK.into_response(),
        Ok(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn admin_status(State(state): State<AppState>) -> impl IntoResponse {
    match state.admin.status() {
        Ok(status) => Json(status).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn link_device(State(state): State<AppState>) -> impl IntoResponse {
    match state.admin.link_device().await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn events(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
) -> impl IntoResponse {
    if let Some(account) = query.account.as_deref()
        && account != state.cfg.account_id
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let receiver = state.client.subscribe();
    let stream = BroadcastStream::new(receiver).filter_map(|result| match result {
        Ok(event) => {
            let payload = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(
                Event::default()
                    .event("message")
                    .id(event.event_id)
                    .data(payload),
            ))
        }
        Err(_) => None,
    });

    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

async fn rpc(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    if request.jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(
                request.id,
                -32600,
                "jsonrpc must be 2.0",
            )),
        );
    }

    let linked = match state.store.is_linked(&state.cfg.account_id) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(JsonRpcResponse::error(request.id, -32000, err.to_string())),
            );
        }
    };
    if !linked {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(JsonRpcResponse::error(
                request.id,
                -32001,
                "account is not linked",
            )),
        );
    }

    let response = match request.method.as_str() {
        "send" => {
            let params: SendParams = match serde_json::from_value(request.params.clone()) {
                Ok(params) => params,
                Err(err) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(JsonRpcResponse::error(request.id, -32602, err.to_string())),
                    );
                }
            };
            match state.client.send_text(params.clone()).await {
                Ok(receipt) => {
                    let message_id = match state.store.get_or_record_idempotent_send(
                        params.idempotency_key.as_deref(),
                        &receipt.message_id,
                    ) {
                        Ok(message_id) => message_id,
                        Err(err) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(JsonRpcResponse::error(request.id, -32000, err.to_string())),
                            );
                        }
                    };
                    let _ = state
                        .store
                        .update_last_sync(&state.cfg.account_id, Utc::now());
                    JsonRpcResponse::ok(
                        request.id,
                        json!({ "accepted": receipt.accepted, "message_id": message_id }),
                    )
                }
                Err(err) => JsonRpcResponse::error(request.id, -32010, err.to_string()),
            }
        }
        "sendAttachment" => {
            let params: SendAttachmentParams = match serde_json::from_value(request.params.clone())
            {
                Ok(params) => params,
                Err(err) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(JsonRpcResponse::error(request.id, -32602, err.to_string())),
                    );
                }
            };
            let params = SendAttachmentParams {
                attachments: params
                    .attachments
                    .into_iter()
                    .take(8)
                    .collect::<Vec<AttachmentRef>>(),
                ..params
            };
            match state.client.send_attachment(params.clone()).await {
                Ok(receipt) => {
                    let message_id = match state.store.get_or_record_idempotent_send(
                        params.idempotency_key.as_deref(),
                        &receipt.message_id,
                    ) {
                        Ok(message_id) => message_id,
                        Err(err) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(JsonRpcResponse::error(request.id, -32000, err.to_string())),
                            );
                        }
                    };
                    let _ = state
                        .store
                        .update_last_sync(&state.cfg.account_id, Utc::now());
                    JsonRpcResponse::ok(
                        request.id,
                        json!({ "accepted": receipt.accepted, "message_id": message_id }),
                    )
                }
                Err(err) => JsonRpcResponse::error(request.id, -32011, err.to_string()),
            }
        }
        "markRead" => {
            let params: MarkReadParams = match serde_json::from_value(request.params.clone()) {
                Ok(params) => params,
                Err(err) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(JsonRpcResponse::error(request.id, -32602, err.to_string())),
                    );
                }
            };
            match state.client.mark_read(params).await {
                Ok(()) => JsonRpcResponse::ok(request.id, json!({ "accepted": true })),
                Err(err) => JsonRpcResponse::error(request.id, -32012, err.to_string()),
            }
        }
        _ => JsonRpcResponse::error(request.id, -32601, "method not found"),
    };

    (StatusCode::OK, Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use gateway_signal::MockSignalClient;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    fn test_app(db_name: &str) -> Router {
        let cfg = GatewayConfig {
            store_path: std::env::temp_dir()
                .join(format!(
                    "signal-gatewayd-{db_name}-{}.db",
                    std::process::id()
                ))
                .display()
                .to_string(),
            ..GatewayConfig::default()
        };
        let store = GatewayStore::open(&cfg.store_path).expect("open store");
        store.ensure_account(&cfg).expect("ensure account");
        let client: Arc<dyn SignalClient> = Arc::new(MockSignalClient::new(cfg.account_id.clone()));
        router(AppState::new(cfg, store, client))
    }

    #[tokio::test]
    async fn ready_requires_linked_account() {
        let app = test_app("ready");
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn link_then_send_is_idempotent() {
        let app = test_app("rpc");

        let link_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/link-device")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("response");
        assert_eq!(link_response.status(), StatusCode::OK);

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "send",
            "params": {
                "account_id": "default",
                "conversation_id": "chat:test",
                "text": "hello",
                "idempotency_key": "same-key"
            }
        });

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/rpc")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("build request"),
            )
            .await
            .expect("response");
        assert_eq!(first.status(), StatusCode::OK);
        let first_json: serde_json::Value =
            serde_json::from_slice(&first.into_body().collect().await.expect("body").to_bytes())
                .expect("json");

        let second = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/rpc")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("build request"),
            )
            .await
            .expect("response");
        assert_eq!(second.status(), StatusCode::OK);
        let second_json: serde_json::Value =
            serde_json::from_slice(&second.into_body().collect().await.expect("body").to_bytes())
                .expect("json");

        assert_eq!(
            first_json["result"]["message_id"],
            second_json["result"]["message_id"]
        );
    }
}
