use crate::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;

#[derive(Clone)]
pub struct GrpcRateLimiter {
    redis: ReloadableRedisPool,
    max_rpm: u64,
}

impl GrpcRateLimiter {
    pub fn new(redis: ReloadableRedisPool, max_rpm: u64) -> Self {
        Self { redis, max_rpm }
    }

    pub async fn check(&self, caller: &str) -> Result<(), tonic::Status> {
        let minute = chrono::Utc::now().format("%Y%m%d%H%M").to_string();
        let key = format!("grpc:ratelimit:{caller}:{minute}");
        let mut conn = self
            .redis
            .get()
            .await
            .map_err(|e| tonic::Status::unavailable(format!("redis: {e}")))?;
        let count: u64 = conn
            .incr(&key, 1u64)
            .await
            .map_err(|e| tonic::Status::unavailable(format!("redis: {e}")))?;
        if count == 1 {
            let _: () = conn
                .expire(&key, 90i64)
                .await
                .map_err(|e| tonic::Status::unavailable(format!("redis: {e}")))?;
        }
        if count > self.max_rpm {
            return Err(tonic::Status::resource_exhausted(format!(
                "rate limit exceeded ({}/min) for caller {caller}",
                self.max_rpm
            )));
        }
        Ok(())
    }
}
