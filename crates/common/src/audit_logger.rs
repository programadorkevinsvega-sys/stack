use sqlx::PgPool;
use uuid::Uuid;

use crate::AuditSigner;

#[derive(Clone)]
pub struct PgAuditLogger {
    pub pool: PgPool,
    pub signer: AuditSigner,
}

pub struct AuditEntry {
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub ip_address: Option<String>,
}

impl PgAuditLogger {
    pub async fn record(&self, entry: AuditEntry) -> Result<(), String> {
        let metadata_str = serde_json::to_string(&entry.metadata).map_err(|e| e.to_string())?;
        let sig = self
            .signer
            .sign(
                &entry.actor_type,
                entry.actor_id,
                &entry.action,
                &entry.resource_type,
                entry.resource_id,
                Some(&metadata_str),
            )
            .map_err(|e| e.to_string())?;

        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (id, actor_type, actor_id, action, resource_type, resource_id, metadata, ip_address, signature, created_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::inet, $9, now())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(&entry.actor_type)
        .bind(entry.actor_id)
        .bind(&entry.action)
        .bind(&entry.resource_type)
        .bind(entry.resource_id)
        .bind(&entry.metadata)
        .bind(&entry.ip_address)
        .bind(&sig)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(())
    }
}
