use crate::domain::entities::PlanLimits;
use crate::domain::errors::SubscriptionError;
use crate::domain::repositories::PlanLimitsCache;
use async_trait::async_trait;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use futures_util::StreamExt;
use uuid::Uuid;
pub struct RedisPlanLimitsCache {
    pub pool: ReloadableRedisPool,
    pub ttl: u64,
}
#[async_trait]
impl PlanLimitsCache for RedisPlanLimitsCache {
    async fn get(&self, user_id: Uuid) -> Result<Option<PlanLimits>, SubscriptionError> {
        let mut c = self
            .pool
            .get()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let value: Option<String> = c
            .get(format!("subscription:limits:{user_id}"))
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        value
            .map(|v| {
                serde_json::from_str(&v).map_err(|e| SubscriptionError::Validation(e.to_string()))
            })
            .transpose()
    }
    async fn set(&self, user_id: Uuid, limits: &PlanLimits) -> Result<(), SubscriptionError> {
        let mut c = self
            .pool
            .get()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let value = serde_json::to_string(limits)
            .map_err(|e| SubscriptionError::Validation(e.to_string()))?;
        c.set_ex::<_, _, ()>(format!("subscription:limits:{user_id}"), value, self.ttl)
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
    async fn invalidate(&self, user_id: Uuid) -> Result<(), SubscriptionError> {
        let mut c = self
            .pool
            .get()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        c.del::<_, ()>(format!("subscription:limits:{user_id}"))
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        // key-services shares Redis with subscription-service. Bumping this
        // epoch makes cached API-key scope/policy decisions stale immediately
        // after a plan or subscription transition.
        let _: u64 = c
            .incr(format!("keys:authz:version:{user_id}"), 1u64)
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
    async fn invalidate_plan(&self, plan_id: Uuid) -> Result<(), SubscriptionError> {
        let mut c = self
            .pool
            .get()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let mut iter = c
            .scan_match::<_, String>("subscription:limits:*")
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let mut keys = Vec::new();
        while let Some(key) = iter.next().await {
            keys.push(key.map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?);
        }
        drop(iter);
        for key in keys {
            let value: Option<String> = c
                .get(&key)
                .await
                .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
            if value
                .as_deref()
                .and_then(|v| serde_json::from_str::<PlanLimits>(v).ok())
                .map(|v| v.plan_id == plan_id)
                .unwrap_or(false)
            {
                if let Some(user_id) = key
                    .strip_prefix("subscription:limits:")
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
                {
                    let _: u64 = c
                        .incr(format!("keys:authz:version:{user_id}"), 1u64)
                        .await
                        .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
                }
                let _: () = c
                    .del(key)
                    .await
                    .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
            }
        }
        Ok(())
    }
}
