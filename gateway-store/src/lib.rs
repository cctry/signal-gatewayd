use anyhow::Context;
use chrono::{DateTime, Utc};
use gateway_core::{AdminStatus, GatewayConfig, GatewayHealth, LinkDeviceResponse};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct GatewayStore {
    inner: Arc<Mutex<Connection>>,
}

impl GatewayStore {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let create_parent = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty());
        if let Some(parent) = create_parent {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create store parent {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite store at {path}"))?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS account (
                account_id TEXT PRIMARY KEY,
                linked INTEGER NOT NULL DEFAULT 0,
                linked_uri TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS checkpoints (
                account_id TEXT PRIMARY KEY,
                last_successful_sync TEXT
            );

            CREATE TABLE IF NOT EXISTS outbound_requests (
                idempotency_key TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )?;

        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn ensure_account(&self, cfg: &GatewayConfig) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.inner.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO account(account_id, linked, updated_at) VALUES(?1, 0, ?2)
             ON CONFLICT(account_id) DO NOTHING",
            params![cfg.account_id, now],
        )?;
        Ok(())
    }

    pub fn get_health(
        &self,
        cfg: &GatewayConfig,
        receive_loop_live: bool,
    ) -> anyhow::Result<GatewayHealth> {
        let conn = self.inner.lock().expect("sqlite mutex poisoned");
        let linked = conn
            .query_row(
                "SELECT linked FROM account WHERE account_id = ?1",
                params![cfg.account_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0;
        let last_successful_sync = conn
            .query_row(
                "SELECT last_successful_sync FROM checkpoints WHERE account_id = ?1",
                params![cfg.account_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .and_then(|raw| DateTime::parse_from_rfc3339(&raw).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(GatewayHealth {
            ok: true,
            account_id: cfg.account_id.clone(),
            linked,
            receive_loop_live,
            last_successful_sync,
            version: cfg.version.clone(),
        })
    }

    pub fn get_admin_status(
        &self,
        cfg: &GatewayConfig,
        receive_loop_live: bool,
    ) -> anyhow::Result<AdminStatus> {
        let health = self.get_health(cfg, receive_loop_live)?;
        Ok(AdminStatus {
            account_id: health.account_id,
            linked: health.linked,
            storage_path: cfg.store_path.clone(),
            receive_loop_live,
            version: cfg.version.clone(),
        })
    }

    pub fn mark_linked(&self, cfg: &GatewayConfig) -> anyhow::Result<LinkDeviceResponse> {
        let uri = format!("sgd://link/{}", cfg.account_id);
        let now = Utc::now().to_rfc3339();
        let conn = self.inner.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO account(account_id, linked, linked_uri, updated_at)
             VALUES(?1, 1, ?2, ?3)
             ON CONFLICT(account_id) DO UPDATE SET linked = 1, linked_uri = excluded.linked_uri, updated_at = excluded.updated_at",
            params![cfg.account_id, uri, now],
        )?;
        Ok(LinkDeviceResponse {
            account_id: cfg.account_id.clone(),
            linked: true,
            uri,
        })
    }

    pub fn is_linked(&self, account_id: &str) -> anyhow::Result<bool> {
        let conn = self.inner.lock().expect("sqlite mutex poisoned");
        let linked = conn
            .query_row(
                "SELECT linked FROM account WHERE account_id = ?1",
                params![account_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0;
        Ok(linked)
    }

    pub fn update_last_sync(&self, account_id: &str, at: DateTime<Utc>) -> anyhow::Result<()> {
        let conn = self.inner.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO checkpoints(account_id, last_successful_sync)
             VALUES(?1, ?2)
             ON CONFLICT(account_id) DO UPDATE SET last_successful_sync = excluded.last_successful_sync",
            params![account_id, at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_or_record_idempotent_send(
        &self,
        key: Option<&str>,
        message_id: &str,
    ) -> anyhow::Result<String> {
        let Some(key) = key else {
            return Ok(message_id.to_string());
        };

        let conn = self.inner.lock().expect("sqlite mutex poisoned");
        if let Some(existing) = conn
            .query_row(
                "SELECT message_id FROM outbound_requests WHERE idempotency_key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing);
        }

        conn.execute(
            "INSERT INTO outbound_requests(idempotency_key, message_id, created_at)
             VALUES(?1, ?2, ?3)",
            params![key, message_id, Utc::now().to_rfc3339()],
        )?;
        Ok(message_id.to_string())
    }
}
