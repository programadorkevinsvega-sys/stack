use crate::application::ports::AuditLogger;
use crate::domain::entities::{AwsCredential, MantleConnection};
use crate::domain::errors::CredentialError;
use crate::domain::repositories::{AwsAccountModelRepository, AwsCredentialRepository};
use async_trait::async_trait;
use common::{AuditEntry, PgAuditLogger};
use events::{
    outbox::enqueue_in_transaction, AccountMarkedUnhealthy, AccountRecovered, EventEnvelope,
    EVENT_ACCOUNT_MARKED_UNHEALTHY, EVENT_ACCOUNT_RECOVERED,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgAuditLoggerAdapter {
    inner: PgAuditLogger,
}

impl PgAuditLoggerAdapter {
    pub fn new(inner: PgAuditLogger) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AuditLogger for PgAuditLoggerAdapter {
    async fn record(&self, entry: AuditEntry) -> Result<(), CredentialError> {
        self.inner
            .record(entry)
            .await
            .map_err(CredentialError::DependencyUnavailable)
    }
}

fn db_err(e: sqlx::Error) -> CredentialError {
    CredentialError::DependencyUnavailable(format!("postgres error: {e}"))
}

pub struct PgAwsCredentialRepository {
    pub pool: PgPool,
}

#[async_trait]
impl AwsCredentialRepository for PgAwsCredentialRepository {
    async fn save(&self, cred: &AwsCredential) -> Result<bool, CredentialError> {
        cred.validate()?;
        let result = sqlx::query!(
            r#"
            INSERT INTO aws_credential_pool (
                id, account_alias, aws_account_id, region, auth_method, role_arn, external_id,
                access_key_encrypted, secret_key_encrypted, kms_key_id,
                max_tpm, max_rpm, priority_weight, is_healthy, consecutive_errors,
                last_error_at, last_error_code, last_health_check_at, is_active, created_at
            )
            VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                $11,$12,$13,$14,$15,$16,$17,$18,$19,$20
            )
            ON CONFLICT (aws_account_id) DO NOTHING
            "#,
            cred.id,
            &cred.account_alias,
            &cred.aws_account_id,
            &cred.region,
            &cred.auth_method,
            cred.role_arn.as_deref(),
            cred.external_id.as_deref(),
            cred.access_key_encrypted.as_deref(),
            cred.secret_key_encrypted.as_deref(),
            cred.kms_key_id.as_deref(),
            cred.max_tpm,
            cred.max_rpm,
            cred.priority_weight,
            cred.is_healthy,
            cred.consecutive_errors,
            cred.last_error_at,
            cred.last_error_code.as_deref(),
            cred.last_health_check_at,
            cred.is_active,
            cred.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<AwsCredential>, CredentialError> {
        let cred = sqlx::query_as::<_, AwsCredential>(
            r#"
            SELECT
                id, account_alias, aws_account_id, region, auth_method, role_arn, external_id,
                access_key_encrypted, secret_key_encrypted, kms_key_id,
                max_tpm, max_rpm, priority_weight, is_healthy, consecutive_errors,
                last_error_at, last_error_code, last_health_check_at, is_active, created_at
            FROM aws_credential_pool
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        if let Some(ref c) = cred {
            c.validate()?;
        }
        Ok(cred)
    }

    async fn find_by_aws_account_id(
        &self,
        aws_account_id: &str,
    ) -> Result<Option<AwsCredential>, CredentialError> {
        let cred = sqlx::query_as::<_, AwsCredential>(
            r#"
            SELECT
                id, account_alias, aws_account_id, region, auth_method, role_arn, external_id,
                access_key_encrypted, secret_key_encrypted, kms_key_id,
                max_tpm, max_rpm, priority_weight, is_healthy, consecutive_errors,
                last_error_at, last_error_code, last_health_check_at, is_active, created_at
            FROM aws_credential_pool
            WHERE aws_account_id = $1
            "#,
        )
        .bind(aws_account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        if let Some(ref c) = cred {
            c.validate()?;
        }
        Ok(cred)
    }

    async fn find_healthy_accounts_for_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<AwsCredential>, CredentialError> {
        let accounts = sqlx::query_as::<_, AwsCredential>(
            r#"
            SELECT
                c.id, c.account_alias, c.aws_account_id, c.region, c.auth_method, c.role_arn, c.external_id,
                c.access_key_encrypted, c.secret_key_encrypted, c.kms_key_id,
                c.max_tpm, c.max_rpm, c.priority_weight, c.is_healthy, c.consecutive_errors,
                c.last_error_at, c.last_error_code, c.last_health_check_at, c.is_active, c.created_at
            FROM aws_credential_pool c
            JOIN aws_account_models m ON c.id = m.account_id
            WHERE m.model_catalog_id = $1 AND c.is_healthy = true AND c.is_active = true
            "#,
        )
        .bind(model_catalog_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        for account in &accounts {
            account.validate()?;
        }

        Ok(accounts)
    }

    async fn find_all_active(&self) -> Result<Vec<AwsCredential>, CredentialError> {
        let accounts = sqlx::query_as::<_, AwsCredential>(
            r#"
            SELECT
                id, account_alias, aws_account_id, region, auth_method, role_arn, external_id,
                access_key_encrypted, secret_key_encrypted, kms_key_id,
                max_tpm, max_rpm, priority_weight, is_healthy, consecutive_errors,
                last_error_at, last_error_code, last_health_check_at, is_active, created_at
            FROM aws_credential_pool
            WHERE is_active = true
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        for account in &accounts {
            account.validate()?;
        }
        Ok(accounts)
    }

    async fn update_health(
        &self,
        id: Uuid,
        is_healthy: bool,
        consecutive_errors: i16,
        error_code: Option<&str>,
    ) -> Result<(), CredentialError> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let previous = sqlx::query_as::<_, (String, bool)>(
            "SELECT aws_account_id,is_healthy FROM aws_credential_pool \
             WHERE id=$1 AND is_active=true FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(db_err)?
        .ok_or_else(|| CredentialError::AccountNotFound(id.to_string()))?;
        let result = sqlx::query!(
            r#"
            UPDATE aws_credential_pool
            SET is_healthy = $2,
                consecutive_errors = $3,
                last_error_code = CASE WHEN $4::text IS NULL THEN NULL ELSE $4 END,
                last_error_at = CASE WHEN $4 IS NULL THEN last_error_at ELSE now() END,
                last_health_check_at = now()
            WHERE id = $1 AND is_active = true
            "#,
            id,
            is_healthy,
            consecutive_errors,
            error_code,
        )
        .execute(tx.as_mut())
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(CredentialError::AccountNotFound(id.to_string()));
        }

        if previous.1 && !is_healthy {
            let envelope = EventEnvelope::new(
                EVENT_ACCOUNT_MARKED_UNHEALTHY,
                AccountMarkedUnhealthy {
                    account_id: id,
                    aws_account_id: previous.0,
                    error_code: error_code.unwrap_or("unhealthy").to_string(),
                    error_message: "unhealthy threshold reached".to_string(),
                },
            );
            enqueue_in_transaction(
                &mut tx,
                "event_outbox",
                EVENT_ACCOUNT_MARKED_UNHEALTHY,
                &envelope,
            )
            .await
            .map_err(|error| CredentialError::DependencyUnavailable(error.to_string()))?;
        } else if !previous.1 && is_healthy {
            let envelope = EventEnvelope::new(
                EVENT_ACCOUNT_RECOVERED,
                AccountRecovered {
                    account_id: id,
                    aws_account_id: previous.0,
                },
            );
            enqueue_in_transaction(&mut tx, "event_outbox", EVENT_ACCOUNT_RECOVERED, &envelope)
                .await
                .map_err(|error| CredentialError::DependencyUnavailable(error.to_string()))?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn update_metadata(
        &self,
        id: Uuid,
        max_tpm: i32,
        max_rpm: i32,
        priority_weight: i16,
    ) -> Result<(), CredentialError> {
        let result = sqlx::query!(
            r#"
            UPDATE aws_credential_pool
            SET max_tpm = $2,
                max_rpm = $3,
                priority_weight = $4
            WHERE id = $1 AND is_active = true
            "#,
            id,
            max_tpm,
            max_rpm,
            priority_weight,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(CredentialError::AccountNotFound(id.to_string()));
        }

        Ok(())
    }

    async fn soft_delete(&self, id: Uuid) -> Result<(), CredentialError> {
        let result = sqlx::query!(
            "UPDATE aws_credential_pool SET is_active = false WHERE id = $1",
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        if result.rows_affected() == 0 {
            return Err(CredentialError::AccountNotFound(id.to_string()));
        }
        Ok(())
    }
}

pub struct PgAwsAccountModelRepository {
    pub pool: PgPool,
}

#[async_trait]
impl AwsAccountModelRepository for PgAwsAccountModelRepository {
    async fn save(&self, account_id: Uuid, model_catalog_id: Uuid) -> Result<(), CredentialError> {
        sqlx::query!(
            r#"
            INSERT INTO aws_account_models (account_id, model_catalog_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
            account_id,
            model_catalog_id,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn delete(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<(), CredentialError> {
        sqlx::query!(
            "DELETE FROM aws_account_models WHERE account_id = $1 AND model_catalog_id = $2",
            account_id,
            model_catalog_id,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn find_models_by_account(&self, account_id: Uuid) -> Result<Vec<Uuid>, CredentialError> {
        let rows = sqlx::query_scalar!(
            "SELECT model_catalog_id FROM aws_account_models WHERE account_id = $1",
            account_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows)
    }

    async fn get_mantle_connection(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<Option<MantleConnection>, CredentialError> {
        sqlx::query_as::<_, MantleConnectionRow>(
            r#"
            SELECT account_id, model_catalog_id, mantle_api_key_encrypted,
                   mantle_api_kms_key_id, mantle_base_url, mantle_expires_at
            FROM aws_account_models
            WHERE account_id = $1 AND model_catalog_id = $2
            "#,
        )
        .bind(account_id)
        .bind(model_catalog_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)
        .map(|row| row.map(Into::into))
    }

    async fn save_mantle_connection(
        &self,
        connection: &MantleConnection,
    ) -> Result<(), CredentialError> {
        let result = sqlx::query(
            r#"
            INSERT INTO aws_account_models (
                account_id, model_catalog_id, mantle_api_key_encrypted,
                mantle_api_kms_key_id, mantle_base_url, mantle_expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6)
            ON CONFLICT (account_id, model_catalog_id) DO UPDATE SET
                mantle_api_key_encrypted = EXCLUDED.mantle_api_key_encrypted,
                mantle_api_kms_key_id = EXCLUDED.mantle_api_kms_key_id,
                mantle_base_url = EXCLUDED.mantle_base_url,
                mantle_expires_at = EXCLUDED.mantle_expires_at
            "#,
        )
        .bind(connection.account_id)
        .bind(connection.model_catalog_id)
        .bind(&connection.api_key_encrypted)
        .bind(&connection.api_key_kms_key_id)
        .bind(&connection.base_url)
        .bind(connection.expires_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(CredentialError::DependencyUnavailable(
                "Mantle connection was not persisted".into(),
            ));
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct MantleConnectionRow {
    account_id: Uuid,
    model_catalog_id: Uuid,
    mantle_api_key_encrypted: Option<Vec<u8>>,
    mantle_api_kms_key_id: Option<String>,
    mantle_base_url: Option<String>,
    mantle_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<MantleConnectionRow> for MantleConnection {
    fn from(row: MantleConnectionRow) -> Self {
        Self {
            account_id: row.account_id,
            model_catalog_id: row.model_catalog_id,
            api_key_encrypted: row.mantle_api_key_encrypted,
            api_key_kms_key_id: row.mantle_api_kms_key_id,
            base_url: row.mantle_base_url,
            expires_at: row.mantle_expires_at,
        }
    }
}
