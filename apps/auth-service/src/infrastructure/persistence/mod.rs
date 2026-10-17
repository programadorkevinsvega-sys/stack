use crate::application::ports::{AuditEntry, AuditLogger};
use crate::domain::entities::{EmailVerification, PasswordReset, RefreshToken, User};
use crate::domain::errors::AuthError;
use crate::domain::repositories::{
    EmailVerificationRepository, LockResult, PasswordResetRepository, RefreshTokenRepository,
    UserRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::KeysetPagination;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgUserRepository {
    pub pool: PgPool,
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn save(&self, u: &User) -> Result<(), AuthError> {
        sqlx::query(
            r#"INSERT INTO users
            (id, email, password_hash, display_name, avatar_url, email_verified_at,
             mfa_enabled, mfa_secret_encrypted, status, role, failed_login_count, locked_until,
             last_login_at, last_login_ip, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"#,
        )
        .bind(u.id)
        .bind(&u.email)
        .bind(&u.password_hash)
        .bind(u.display_name.as_deref())
        .bind(u.avatar_url.as_deref())
        .bind(u.email_verified_at)
        .bind(u.mfa_enabled)
        .bind(u.mfa_secret_encrypted.as_deref())
        .bind(&u.status)
        .bind(&u.role)
        .bind(u.failed_login_count)
        .bind(u.locked_until)
        .bind(u.last_login_at)
        .bind(
            u.last_login_ip
                .as_ref()
                .and_then(|ip| ip.parse::<sqlx::types::ipnetwork::IpNetwork>().ok()),
        )
        .bind(u.created_at)
        .bind(u.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AuthError::Validation(e.to_string()))
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AuthError::Validation(e.to_string()))
    }

    async fn find_all(
        &self,
        user_id: Option<Uuid>,
        limit: u32,
        after_cursor: Option<String>,
    ) -> Result<(Vec<User>, Option<String>), AuthError> {
        let limit = limit.min(100);

        let base_sql = if user_id.is_some() {
            "SELECT * FROM users WHERE id = $1"
        } else {
            "SELECT * FROM users WHERE 1=1"
        };

        let cursor_sql = if let Some(ref cursor) = after_cursor {
            if let Some((cursor_time, cursor_id)) = KeysetPagination::decode_cursor(cursor) {
                format!(
                    " AND (created_at, id) < ('{cursor_time}'::timestamptz, '{cursor_id}'::uuid)"
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let sql = format!(
            "{base_sql}{cursor_sql} ORDER BY created_at DESC, id DESC LIMIT ${}",
            if user_id.is_some() { 2 } else { 1 }
        );

        let results = if let Some(uid) = user_id {
            sqlx::query_as::<_, User>(&sql)
                .bind(uid)
                .bind(i64::from(limit + 1))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?
        } else {
            sqlx::query_as::<_, User>(&sql)
                .bind(i64::from(limit + 1))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?
        };

        let has_more = results.len() > limit as usize;
        let users: Vec<User> = if has_more {
            results.into_iter().take(limit as usize).collect()
        } else {
            results
        };

        let next_cursor = users
            .last()
            .map(|u| KeysetPagination::encode_cursor(u.created_at, u.id));

        Ok((users, if has_more { next_cursor } else { None }))
    }

    async fn update(&self, u: &User) -> Result<(), AuthError> {
        sqlx::query(
            r#"UPDATE users SET email=$2, password_hash=$3, display_name=$4,
            avatar_url=$5, email_verified_at=$6, mfa_enabled=$7, mfa_secret_encrypted=$8,
            status=$9, role=$10, failed_login_count=$11, locked_until=$12, last_login_at=$13,
            last_login_ip=$14, updated_at=now() WHERE id=$1"#,
        )
        .bind(u.id)
        .bind(&u.email)
        .bind(&u.password_hash)
        .bind(u.display_name.as_deref())
        .bind(u.avatar_url.as_deref())
        .bind(u.email_verified_at)
        .bind(u.mfa_enabled)
        .bind(u.mfa_secret_encrypted.as_deref())
        .bind(&u.status)
        .bind(&u.role)
        .bind(u.failed_login_count)
        .bind(u.locked_until)
        .bind(u.last_login_at)
        .bind(
            u.last_login_ip
                .as_ref()
                .and_then(|ip| ip.parse::<sqlx::types::ipnetwork::IpNetwork>().ok()),
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn anonymize(&self, id: Uuid) -> Result<(), AuthError> {
        let placeholder = format!("deleted+{id}@deleted.local");
        sqlx::query(
            "UPDATE users SET email = $2, password_hash = '', display_name = NULL, avatar_url = NULL,
             email_verified_at = NULL, mfa_enabled = false, mfa_secret_encrypted = NULL,
             status = 'disabled', failed_login_count = 0, locked_until = NULL,
             last_login_at = NULL, last_login_ip = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(placeholder)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn increment_failed_login(&self, id: Uuid) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE users SET failed_login_count = failed_login_count + 1 WHERE id = $1",
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn increment_failed_login_and_maybe_lock(
        &self,
        id: Uuid,
        max_attempts: i32,
        lock_duration_minutes: i32,
    ) -> Result<LockResult, AuthError> {
        let row = sqlx::query!(
            "UPDATE users
             SET failed_login_count = failed_login_count + 1,
                 locked_until = CASE
                     WHEN failed_login_count + 1 >= $2 THEN now() + ($3::int4 || ' minutes')::interval
                     ELSE locked_until
                 END,
                 status = CASE
                     WHEN failed_login_count + 1 >= $2 THEN 'locked'
                     ELSE status
                 END
             WHERE id = $1
             RETURNING failed_login_count::int4 AS \"new_failed_count!\", locked_until, status",
            id,
            max_attempts,
            lock_duration_minutes,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(LockResult {
            was_locked: row.status == "locked",
            new_failed_count: row.new_failed_count,
            locked_until: row.locked_until,
        })
    }

    async fn reset_failed_login(&self, id: Uuid) -> Result<(), AuthError> {
        sqlx::query!("UPDATE users SET failed_login_count = 0, locked_until = NULL, last_login_at = now() WHERE id = $1", id,)
            .execute(&self.pool).await
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn lock_account(&self, id: Uuid, until: DateTime<Utc>) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE users SET locked_until = $2, status = 'locked' WHERE id = $1",
            id,
            until
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

pub struct PgRefreshTokenRepository {
    pub pool: PgPool,
}

#[async_trait]
impl RefreshTokenRepository for PgRefreshTokenRepository {
    async fn save(&self, t: &RefreshToken) -> Result<(), AuthError> {
        sqlx::query!("INSERT INTO refresh_tokens (id, user_id, token_hash, user_agent, ip_address, expires_at, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
            t.id, t.user_id, &t.token_hash, t.user_agent.as_deref(),
            t.ip_address.as_ref().and_then(|ip| ip.parse::<sqlx::types::ipnetwork::IpNetwork>().ok()), t.expires_at, t.created_at)
            .execute(&self.pool).await
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<RefreshToken>, AuthError> {
        sqlx::query_as::<_, RefreshToken>("SELECT * FROM refresh_tokens WHERE token_hash = $1")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AuthError::Validation(e.to_string()))
    }

    async fn revoke(&self, id: Uuid) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE refresh_tokens SET revoked_at = now() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), AuthError> {
        sqlx::query!("UPDATE refresh_tokens SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
            user_id).execute(&self.pool).await
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

pub struct PgPasswordResetRepository {
    pub pool: PgPool,
}

#[async_trait]
impl PasswordResetRepository for PgPasswordResetRepository {
    async fn save(&self, r: &PasswordReset) -> Result<(), AuthError> {
        sqlx::query!("INSERT INTO password_resets (id, user_id, token_hash, expires_at, created_at) VALUES ($1,$2,$3,$4,$5)",
            r.id, r.user_id, &r.token_hash, r.expires_at, r.created_at)
            .execute(&self.pool).await
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<PasswordReset>, AuthError> {
        sqlx::query_as::<_, PasswordReset>("SELECT * FROM password_resets WHERE token_hash = $1")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AuthError::Validation(e.to_string()))
    }

    async fn mark_used(&self, id: Uuid) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE password_resets SET used_at = now() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

pub struct PgEmailVerificationRepository {
    pub pool: PgPool,
}

#[async_trait]
impl EmailVerificationRepository for PgEmailVerificationRepository {
    async fn save(&self, v: &EmailVerification) -> Result<(), AuthError> {
        sqlx::query!("INSERT INTO email_verifications (id, user_id, token_hash, expires_at, verified_at, created_at) VALUES ($1,$2,$3,$4,$5,$6)",
            v.id,
            v.user_id,
            &v.token_hash,
            v.expires_at,
            v.verified_at,
            v.created_at)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<EmailVerification>, AuthError> {
        sqlx::query_as::<_, EmailVerification>(
            "SELECT * FROM email_verifications WHERE token_hash = $1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Validation(e.to_string()))
    }

    async fn mark_verified(&self, id: Uuid) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE email_verifications SET verified_at = now() WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn revoke_pending_for_user(&self, user_id: Uuid) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE email_verifications SET expires_at = now() WHERE user_id = $1 AND verified_at IS NULL AND expires_at > now()",
            user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

pub struct PgAuditLogger {
    pub pool: PgPool,
    pub signer: common::AuditSigner,
}

#[async_trait]
impl AuditLogger for PgAuditLogger {
    async fn record(&self, entry: AuditEntry) -> Result<(), AuthError> {
        let metadata_str = serde_json::to_string(&entry.metadata)
            .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        let sig = self
            .signer
            .sign(
                entry.actor_type.as_str(),
                entry.actor_id,
                &entry.action,
                &entry.resource_type,
                entry.resource_id,
                Some(&metadata_str),
            )
            .map_err(AuthError::DependencyUnavailable)?;
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (id, actor_type, actor_id, action, resource_type, resource_id, metadata, ip_address, signature, created_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS inet), $9, now())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(entry.actor_type.as_str())
        .bind(entry.actor_id)
        .bind(entry.action)
        .bind(entry.resource_type)
        .bind(entry.resource_id)
        .bind(entry.metadata)
        .bind(entry.ip_address.map(|ip| ip.to_string()))
        .bind(sig)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}
