use crate::domain::entities::{AccountLoad, CircuitStatus};
use crate::domain::errors::ModelRouterError;
use chrono::Utc;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisRouterState {
    pub pool: ReloadableRedisPool,
}

impl RedisRouterState {
    pub async fn record_selection(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        estimated_tokens: i64,
    ) -> Result<(), ModelRouterError> {
        let mut conn = self.conn().await?;
        let minute = current_minute();
        let active_key = active_key(account_id);
        let rpm_key = rpm_key(account_id, minute);
        let tpm_key = tpm_key(account_id, minute);
        let request_key = request_key(request_id);
        let is_new: bool = conn
            .set_nx(&request_key, account_id.to_string())
            .await
            .map_err(redis_err)?;
        if !is_new {
            return Ok(());
        }
        let _: () = conn.expire(&request_key, 3600).await.map_err(redis_err)?;
        let _: i64 = conn.incr(active_key.clone(), 1).await.map_err(redis_err)?;
        let _: bool = conn.expire(active_key, 3600).await.map_err(redis_err)?;
        let _: i64 = conn.incr(rpm_key.clone(), 1).await.map_err(redis_err)?;
        let _: bool = conn.expire(rpm_key, 120).await.map_err(redis_err)?;
        if estimated_tokens > 0 {
            let _: i64 = conn
                .incr(tpm_key.clone(), estimated_tokens)
                .await
                .map_err(redis_err)?;
            let _: bool = conn.expire(tpm_key, 120).await.map_err(redis_err)?;
        }
        Ok(())
    }

    pub async fn record_outcome(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        actual_tokens: i64,
    ) -> Result<(), ModelRouterError> {
        let mut conn = self.conn().await?;
        let _: i64 = conn.del(request_key(request_id)).await.map_err(redis_err)?;
        let active_key = active_key(account_id);
        let active: i64 = conn.decr(active_key.clone(), 1).await.map_err(redis_err)?;
        if active < 0 {
            let _: () = conn.set(active_key, 0).await.map_err(redis_err)?;
        }
        if actual_tokens > 0 {
            let tpm_key = tpm_key(account_id, current_minute());
            let _: i64 = conn
                .incr(tpm_key.clone(), actual_tokens)
                .await
                .map_err(redis_err)?;
            let _: bool = conn.expire(tpm_key, 120).await.map_err(redis_err)?;
        }
        Ok(())
    }

    pub async fn get_load(&self, account_id: Uuid) -> Result<AccountLoad, ModelRouterError> {
        let mut conn = self.conn().await?;
        let minute = current_minute();
        let active_connections: i64 = conn.get(active_key(account_id)).await.unwrap_or_default();
        let current_tpm: i64 = conn
            .get(tpm_key(account_id, minute))
            .await
            .unwrap_or_default();
        let current_rpm: i64 = conn
            .get(rpm_key(account_id, minute))
            .await
            .unwrap_or_default();
        Ok(AccountLoad {
            account_id,
            active_connections: active_connections.max(0),
            current_tpm: current_tpm.max(0),
            current_rpm: current_rpm.max(0),
        })
    }

    pub async fn is_circuit_open(&self, account_id: Uuid) -> Result<bool, ModelRouterError> {
        let mut conn = self.conn().await?;
        let open_until: Option<i64> = conn.get(circuit_key(account_id)).await.map_err(redis_err)?;
        Ok(open_until
            .map(|until| until > Utc::now().timestamp())
            .unwrap_or(false))
    }

    pub async fn record_failure(&self, account_id: Uuid) -> Result<i64, ModelRouterError> {
        let mut conn = self.conn().await?;
        let key = failure_key(account_id);
        let failures: i64 = conn.incr(key.clone(), 1).await.map_err(redis_err)?;
        let _: bool = conn.expire(key, 3600).await.map_err(redis_err)?;
        Ok(failures)
    }

    pub async fn open_circuit(
        &self,
        account_id: Uuid,
        cooldown_seconds: i64,
    ) -> Result<bool, ModelRouterError> {
        let mut conn = self.conn().await?;
        let key = circuit_key(account_id);
        let already_open: Option<i64> = conn.get(key.clone()).await.map_err(redis_err)?;
        let until = Utc::now().timestamp() + cooldown_seconds.max(1);
        let _: () = conn
            .set_ex(key, until, cooldown_seconds.max(1) as u64)
            .await
            .map_err(redis_err)?;
        Ok(already_open.is_none())
    }

    pub async fn force_close_circuit(&self, account_id: Uuid) -> Result<bool, ModelRouterError> {
        let mut conn = self.conn().await?;
        let existed: i64 = conn.del(circuit_key(account_id)).await.map_err(redis_err)?;
        let _: i64 = conn.del(failure_key(account_id)).await.map_err(redis_err)?;
        Ok(existed > 0)
    }

    pub async fn circuit_status(
        &self,
        account_id: Uuid,
    ) -> Result<CircuitStatus, ModelRouterError> {
        let mut conn = self.conn().await?;
        let open_until: Option<i64> = conn.get(circuit_key(account_id)).await.map_err(redis_err)?;
        let consecutive_failures: i64 = conn.get(failure_key(account_id)).await.unwrap_or_default();
        let now = Utc::now().timestamp();
        let (state, until) = match open_until {
            Some(until) if until > now => ("open".to_string(), until),
            _ if consecutive_failures > 0 => ("closed_with_failures".to_string(), 0),
            _ => ("closed".to_string(), 0),
        };
        Ok(CircuitStatus {
            account_id,
            state,
            open_until_epoch_seconds: until,
            consecutive_failures,
        })
    }

    async fn conn(&self) -> Result<deadpool_redis::Connection, ModelRouterError> {
        self.pool
            .get()
            .await
            .map_err(|e| ModelRouterError::DependencyUnavailable(e.to_string()))
    }
}

fn redis_err(error: deadpool_redis::redis::RedisError) -> ModelRouterError {
    ModelRouterError::DependencyUnavailable(format!("redis error: {error}"))
}

fn current_minute() -> i64 {
    Utc::now().timestamp() / 60
}

fn active_key(account_id: Uuid) -> String {
    format!("model_router:active:{account_id}")
}

fn rpm_key(account_id: Uuid, minute: i64) -> String {
    format!("model_router:rpm:{account_id}:{minute}")
}

fn tpm_key(account_id: Uuid, minute: i64) -> String {
    format!("model_router:tpm:{account_id}:{minute}")
}

fn circuit_key(account_id: Uuid) -> String {
    format!("model_router:circuit:{account_id}")
}

fn failure_key(account_id: Uuid) -> String {
    format!("model_router:failures:{account_id}")
}

fn request_key(request_id: Uuid) -> String {
    format!("model_router:request:{request_id}")
}
