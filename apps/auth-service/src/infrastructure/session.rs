use crate::application::ports::{ActiveSession, SessionStore};
use crate::domain::errors::AuthError;
use async_trait::async_trait;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use uuid::Uuid;

const SESSION_KEY_PREFIX: &str = "auth:session:";
const USER_SESSIONS_KEY_PREFIX: &str = "auth:user_sessions:";

pub struct RedisSessionStore {
    pool: ReloadableRedisPool,
}

impl RedisSessionStore {
    pub fn new(pool: ReloadableRedisPool) -> Self {
        Self { pool }
    }

    fn session_key(session_id: Uuid) -> String {
        format!("{SESSION_KEY_PREFIX}{session_id}")
    }

    fn user_sessions_key(user_id: Uuid) -> String {
        format!("{USER_SESSIONS_KEY_PREFIX}{user_id}")
    }

    fn dependency_error(error: impl std::fmt::Display) -> AuthError {
        AuthError::DependencyUnavailable(format!("Redis session store: {error}"))
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn create(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        ttl_seconds: u64,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<(), AuthError> {
        if ttl_seconds == 0 {
            return Err(AuthError::Validation(
                "session ttl must be greater than zero".into(),
            ));
        }
        let mut connection = self.pool.get().await.map_err(Self::dependency_error)?;
        let session_key = Self::session_key(session_id);
        let user_sessions_key = Self::user_sessions_key(user_id);
        let created_at = chrono::Utc::now().to_rfc3339();
        let ttl = i64::try_from(ttl_seconds)
            .map_err(|error| AuthError::Validation(format!("session ttl is too large: {error}")))?;
        deadpool_redis::redis::pipe()
            .atomic()
            .cmd("HSET")
            .arg(&session_key)
            .arg("user_id")
            .arg(user_id.to_string())
            .arg("created_at")
            .arg(created_at)
            .arg("last_seen_at")
            .arg(chrono::Utc::now().to_rfc3339())
            .arg("user_agent")
            .arg(user_agent.unwrap_or_default())
            .arg("ip_address")
            .arg(ip_address.unwrap_or_default())
            .ignore()
            .cmd("EXPIRE")
            .arg(&session_key)
            .arg(ttl)
            .ignore()
            .cmd("SADD")
            .arg(&user_sessions_key)
            .arg(session_id.to_string())
            .ignore()
            .cmd("EXPIRE")
            .arg(&user_sessions_key)
            .arg(ttl)
            .ignore()
            .query_async(&mut *connection)
            .await
            .map_err(Self::dependency_error)
    }

    async fn is_active(&self, session_id: Uuid) -> Result<bool, AuthError> {
        let mut connection = self.pool.get().await.map_err(Self::dependency_error)?;
        let now = chrono::Utc::now().to_rfc3339();
        let active: i64 = deadpool_redis::redis::cmd("EVAL")
            .arg("local exists = redis.call('EXISTS', KEYS[1]); if exists == 1 then redis.call('HSET', KEYS[1], 'last_seen_at', ARGV[1]); return 1 else return 0 end")
            .arg(1usize)
            .arg(Self::session_key(session_id))
            .arg(now)
            .query_async(&mut *connection)
            .await
            .map_err(Self::dependency_error)?;
        Ok(active == 1)
    }

    async fn revoke(&self, session_id: Uuid) -> Result<(), AuthError> {
        let mut connection = self.pool.get().await.map_err(Self::dependency_error)?;
        let session_key = Self::session_key(session_id);
        let user_id: Option<String> = connection
            .hget(&session_key, "user_id")
            .await
            .map_err(Self::dependency_error)?;
        let mut pipeline = deadpool_redis::redis::pipe();
        pipeline.atomic().cmd("DEL").arg(&session_key).ignore();
        if let Some(raw_user_id) = user_id {
            let user_id = Uuid::parse_str(&raw_user_id).map_err(|error| {
                AuthError::DependencyUnavailable(format!("invalid session owner: {error}"))
            })?;
            pipeline
                .cmd("SREM")
                .arg(Self::user_sessions_key(user_id))
                .arg(session_id.to_string())
                .ignore();
        }
        pipeline
            .query_async(&mut *connection)
            .await
            .map_err(Self::dependency_error)
    }

    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), AuthError> {
        let mut connection = self.pool.get().await.map_err(Self::dependency_error)?;
        let user_key = Self::user_sessions_key(user_id);
        let sessions: Vec<String> = connection
            .smembers(&user_key)
            .await
            .map_err(Self::dependency_error)?;
        let mut pipeline = deadpool_redis::redis::pipe();
        pipeline.atomic();
        for session_id in sessions {
            let session_id = Uuid::parse_str(&session_id).map_err(|error| {
                AuthError::DependencyUnavailable(format!("invalid session id: {error}"))
            })?;
            pipeline
                .cmd("DEL")
                .arg(Self::session_key(session_id))
                .ignore();
        }
        pipeline.cmd("DEL").arg(user_key).ignore();
        pipeline
            .query_async(&mut *connection)
            .await
            .map_err(Self::dependency_error)
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<ActiveSession>, AuthError> {
        let mut connection = self.pool.get().await.map_err(Self::dependency_error)?;
        let user_key = Self::user_sessions_key(user_id);
        let session_ids: Vec<String> = connection
            .smembers(&user_key)
            .await
            .map_err(Self::dependency_error)?;
        let owner_id = user_id.to_string();
        let mut sessions = Vec::with_capacity(session_ids.len());
        for raw_session_id in session_ids {
            let session_id = match Uuid::parse_str(&raw_session_id) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(error = %error, "ignoring malformed active session id");
                    continue;
                }
            };
            let session_key = Self::session_key(session_id);
            let owner: Option<String> = connection
                .hget(&session_key, "user_id")
                .await
                .map_err(Self::dependency_error)?;
            if owner.as_deref() != Some(owner_id.as_str()) {
                continue;
            }
            let created_at: Option<String> = connection
                .hget(&session_key, "created_at")
                .await
                .map_err(Self::dependency_error)?;
            let last_seen_at: Option<String> = connection
                .hget(&session_key, "last_seen_at")
                .await
                .map_err(Self::dependency_error)?;
            let Some(created_at) = created_at else {
                continue;
            };
            let Some(last_seen_at) = last_seen_at else {
                continue;
            };
            let user_agent: Option<String> = connection
                .hget(&session_key, "user_agent")
                .await
                .map_err(Self::dependency_error)?;
            let ip_address: Option<String> = connection
                .hget(&session_key, "ip_address")
                .await
                .map_err(Self::dependency_error)?;
            sessions.push(ActiveSession {
                session_id,
                user_agent: user_agent.filter(|value| !value.is_empty()),
                ip_address: ip_address.filter(|value| !value.is_empty()),
                created_at,
                last_seen_at,
            });
        }
        sessions.sort_by(|left, right| right.last_seen_at.cmp(&left.last_seen_at));
        Ok(sessions)
    }

    async fn revoke_for_user(&self, user_id: Uuid, session_id: Uuid) -> Result<bool, AuthError> {
        let mut connection = self.pool.get().await.map_err(Self::dependency_error)?;
        let owner: Option<String> = connection
            .hget(Self::session_key(session_id), "user_id")
            .await
            .map_err(Self::dependency_error)?;
        if owner.as_deref() != Some(user_id.to_string().as_str()) {
            return Ok(false);
        }
        self.revoke(session_id).await?;
        Ok(true)
    }
}
