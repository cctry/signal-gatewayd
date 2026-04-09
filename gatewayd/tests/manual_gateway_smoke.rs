use reqwest::Client;
use serde_json::{Value, json};
use std::env;

fn manual_enabled() -> bool {
    env::var("SIGNAL_GATEWAY_ENABLE_MANUAL").ok().as_deref() == Some("1")
}

fn base_url() -> String {
    env::var("SIGNAL_GATEWAY_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string())
}

fn account_id() -> String {
    env::var("SIGNAL_GATEWAY_ACCOUNT_ID").unwrap_or_else(|_| "default".to_string())
}

fn conversation_id() -> Option<String> {
    env::var("SIGNAL_TEST_CONVERSATION_ID").ok()
}

#[tokio::test]
#[ignore = "manual smoke test against a running gateway instance"]
async fn manual_health_and_ready() {
    if !manual_enabled() {
        eprintln!("set SIGNAL_GATEWAY_ENABLE_MANUAL=1 to run this test");
        return;
    }

    let client = Client::new();

    let health = client
        .get(format!("{}/health", base_url()))
        .send()
        .await
        .expect("health request");
    assert!(
        health.status().is_success(),
        "health status: {}",
        health.status()
    );

    let ready = client
        .get(format!("{}/ready", base_url()))
        .send()
        .await
        .expect("ready request");
    assert!(
        ready.status().as_u16() == 200 || ready.status().as_u16() == 503,
        "ready status: {}",
        ready.status()
    );
}

#[tokio::test]
#[ignore = "manual smoke test against a running gateway instance"]
async fn manual_send_is_idempotent() {
    if !manual_enabled() {
        eprintln!("set SIGNAL_GATEWAY_ENABLE_MANUAL=1 to run this test");
        return;
    }

    let Some(conversation_id) = conversation_id() else {
        eprintln!("set SIGNAL_TEST_CONVERSATION_ID to enable send-path smoke tests");
        return;
    };

    let client = Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "send",
        "params": {
            "account_id": account_id(),
            "conversation_id": conversation_id,
            "text": "signal-gatewayd manual smoke test",
            "idempotency_key": "manual-smoke-key"
        }
    });

    let first: Value = client
        .post(format!("{}/api/v1/rpc", base_url()))
        .json(&body)
        .send()
        .await
        .expect("first request")
        .json()
        .await
        .expect("first json");

    let second: Value = client
        .post(format!("{}/api/v1/rpc", base_url()))
        .json(&body)
        .send()
        .await
        .expect("second request")
        .json()
        .await
        .expect("second json");

    assert_eq!(first["error"], Value::Null, "first response: {first}");
    assert_eq!(second["error"], Value::Null, "second response: {second}");
    assert_eq!(
        first["result"]["message_id"], second["result"]["message_id"],
        "idempotency mismatch: first={first} second={second}"
    );
}
