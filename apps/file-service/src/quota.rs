use chrono::{DateTime, Datelike, Timelike, Utc};
use common::ReloadableRedisPool;
use deadpool_redis::redis;
use sqlx::PgPool;
use uuid::Uuid;

/// Limits resolved from the authenticated user's active subscription.
/// A value of zero means unlimited for rolling usage limits.
#[derive(Clone, Copy, Debug)]
pub struct IngestionLimits {
    pub files_per_hour: i64,
    pub files_per_day: i64,
    pub bytes_per_day: i64,
    pub tokens_per_month: i64,
    pub concurrent: i64,
    pub pending: i64,
}

#[derive(Clone)]
pub struct QuotaManager {
    pool: PgPool,
    redis: ReloadableRedisPool,
}

type UsageWindowRow = (
    DateTime<Utc>,
    DateTime<Utc>,
    DateTime<Utc>,
    i64,
    i64,
    i64,
    i64,
    i64,
);
type UsageRebuildRow = (
    DateTime<Utc>,
    DateTime<Utc>,
    DateTime<Utc>,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
);

const HASH_TTL_SECONDS: i64 = 2_764_800; // 32 days, enough to span all windows.

const RESERVE_SCRIPT: &str = r#"
local hour = ARGV[1]
local day = ARGV[2]
local month = ARGV[3]
if redis.call('HGET', KEYS[1], 'hour_epoch') ~= hour then redis.call('HSET', KEYS[1], 'hour_epoch', hour, 'files_hour', 0) end
if redis.call('HGET', KEYS[1], 'day_epoch') ~= day then redis.call('HSET', KEYS[1], 'day_epoch', day, 'files_day', 0, 'bytes_day', 0) end
if redis.call('HGET', KEYS[1], 'month_epoch') ~= month then redis.call('HSET', KEYS[1], 'month_epoch', month, 'tokens_month', 0) end
local fh = tonumber(redis.call('HGET', KEYS[1], 'files_hour') or '0')
local fd = tonumber(redis.call('HGET', KEYS[1], 'files_day') or '0')
local bd = tonumber(redis.call('HGET', KEYS[1], 'bytes_day') or '0')
local tm = tonumber(redis.call('HGET', KEYS[1], 'tokens_month') or '0')
local pending = tonumber(redis.call('HGET', KEYS[1], 'pending_jobs') or '0')
if tonumber(ARGV[4]) > 0 and fh + 1 > tonumber(ARGV[4]) then return 0 end
if tonumber(ARGV[5]) > 0 and fd + 1 > tonumber(ARGV[5]) then return 0 end
if tonumber(ARGV[6]) > 0 and bd + tonumber(ARGV[7]) > tonumber(ARGV[6]) then return 0 end
if tonumber(ARGV[9]) > 0 and tm + tonumber(ARGV[8]) > tonumber(ARGV[9]) then return 0 end
if tonumber(ARGV[10]) > 0 and pending + 1 > tonumber(ARGV[10]) then return 0 end
redis.call('HINCRBY', KEYS[1], 'files_hour', 1)
redis.call('HINCRBY', KEYS[1], 'files_day', 1)
redis.call('HINCRBY', KEYS[1], 'bytes_day', ARGV[7])
redis.call('HINCRBY', KEYS[1], 'tokens_month', ARGV[8])
redis.call('HINCRBY', KEYS[1], 'pending_jobs', 1)
redis.call('EXPIRE', KEYS[1], 2764800)
return 1
"#;

const RELEASE_SCRIPT: &str = r#"
local function dec(field, value)
  local next = tonumber(redis.call('HINCRBY', KEYS[1], field, -tonumber(value)))
  if next < 0 then redis.call('HSET', KEYS[1], field, 0) end
end
dec('files_hour', ARGV[1]); dec('files_day', ARGV[1]); dec('bytes_day', ARGV[2]); dec('tokens_month', ARGV[3]); dec('pending_jobs', ARGV[4]); dec('concurrent_jobs', ARGV[5]);
redis.call('EXPIRE', KEYS[1], 2764800)
return 1
"#;

const BEGIN_SCRIPT: &str = r#"
local pending = tonumber(redis.call('HGET', KEYS[1], 'pending_jobs') or '0')
local current = tonumber(redis.call('HGET', KEYS[1], 'concurrent_jobs') or '0')
if pending < 1 then return 0 end
if tonumber(ARGV[1]) > 0 and current + 1 > tonumber(ARGV[1]) then return 0 end
redis.call('HINCRBY', KEYS[1], 'pending_jobs', -1)
redis.call('HINCRBY', KEYS[1], 'concurrent_jobs', 1)
redis.call('EXPIRE', KEYS[1], 2764800)
return 1
"#;

const PAUSE_SCRIPT: &str = r#"
local current = tonumber(redis.call('HGET', KEYS[1], 'concurrent_jobs') or '0')
if current > 0 then redis.call('HINCRBY', KEYS[1], 'concurrent_jobs', -1) end
redis.call('HINCRBY', KEYS[1], 'pending_jobs', 1)
redis.call('EXPIRE', KEYS[1], 2764800)
return 1
"#;

const RECONCILE_SCRIPT: &str = r#"
local delta = tonumber(ARGV[1]) - tonumber(ARGV[2])
local tokens = tonumber(redis.call('HINCRBY', KEYS[1], 'tokens_month', delta))
if tokens < 0 then redis.call('HSET', KEYS[1], 'tokens_month', 0) end
local current = tonumber(redis.call('HINCRBY', KEYS[1], 'concurrent_jobs', -1))
if current < 0 then redis.call('HSET', KEYS[1], 'concurrent_jobs', 0) end
redis.call('EXPIRE', KEYS[1], 2764800)
return 1
"#;

impl QuotaManager {
    pub fn new(pool: PgPool, redis: ReloadableRedisPool) -> Self {
        Self { pool, redis }
    }

    fn key(user_id: Uuid) -> String {
        format!("ingestion:quota:{user_id}")
    }

    async fn run_script(&self, source: &str, user_id: Uuid, args: &[i64]) -> Result<bool, String> {
        let mut conn = self.redis.get().await.map_err(|error| error.to_string())?;
        let script = redis::Script::new(source);
        script.key(Self::key(user_id));
        for arg in args {
            script.arg(*arg);
        }
        let result: i32 = script
            .invoke_async(&mut *conn)
            .await
            .map_err(|error| error.to_string())?;
        Ok(result == 1)
    }

    async fn run_reserve(
        &self,
        user_id: Uuid,
        bytes: i64,
        estimated_tokens: i64,
        limits: IngestionLimits,
    ) -> Result<bool, String> {
        let mut conn = self.redis.get().await.map_err(|error| error.to_string())?;
        let now = Utc::now();
        let result: i32 = redis::Script::new(RESERVE_SCRIPT)
            .key(Self::key(user_id))
            .arg(now.format("%Y%m%d%H").to_string())
            .arg(now.format("%Y%m%d").to_string())
            .arg(now.format("%Y%m").to_string())
            .arg(limits.files_per_hour)
            .arg(limits.files_per_day)
            .arg(limits.bytes_per_day)
            .arg(bytes)
            .arg(estimated_tokens)
            .arg(limits.tokens_per_month)
            .arg(limits.pending)
            .invoke_async(&mut *conn)
            .await
            .map_err(|error| error.to_string())?;
        Ok(result == 1)
    }

    /// Rebuilds the low-latency cache from PostgreSQL's authoritative counters.
    pub async fn rebuild_redis(&self, user_id: Uuid) -> Result<(), String> {
        let row: Option<UsageRebuildRow> =
            sqlx::query_as("SELECT hour_started_at,day_started_at,month_started_at,files_hour,files_day,bytes_day,tokens_month,pending_jobs,concurrent_jobs FROM ingestion_quota_usage WHERE user_id=$1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| error.to_string())?;
        let now = Utc::now();
        let (hour, day, month, files_hour, files_day, bytes_day, tokens_month, pending, concurrent) =
            row.map_or_else(
                || {
                    (
                        now.format("%Y%m%d%H").to_string(),
                        now.format("%Y%m%d").to_string(),
                        now.format("%Y%m").to_string(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    )
                },
                |(hour, day, month, fh, fd, bd, tm, pending, concurrent)| {
                    let current_hour =
                        if hour.date_naive() == now.date_naive() && hour.hour() == now.hour() {
                            fh
                        } else {
                            0
                        };
                    let current_day = if day.date_naive() == now.date_naive() {
                        fd
                    } else {
                        0
                    };
                    let current_bytes = if day.date_naive() == now.date_naive() {
                        bd
                    } else {
                        0
                    };
                    let current_month =
                        if month.year() == now.year() && month.month() == now.month() {
                            tm
                        } else {
                            0
                        };
                    (
                        hour.format("%Y%m%d%H").to_string(),
                        day.format("%Y%m%d").to_string(),
                        month.format("%Y%m").to_string(),
                        current_hour,
                        current_day,
                        current_bytes,
                        current_month,
                        pending,
                        concurrent,
                    )
                },
            );
        let mut conn = self.redis.get().await.map_err(|error| error.to_string())?;
        redis::cmd("HSET")
            .arg(Self::key(user_id))
            .arg("hour_epoch")
            .arg(hour)
            .arg("day_epoch")
            .arg(day)
            .arg("month_epoch")
            .arg(month)
            .arg("files_hour")
            .arg(files_hour)
            .arg("files_day")
            .arg(files_day)
            .arg("bytes_day")
            .arg(bytes_day)
            .arg("tokens_month")
            .arg(tokens_month)
            .arg("pending_jobs")
            .arg(pending)
            .arg("concurrent_jobs")
            .arg(concurrent)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|error| error.to_string())?;
        redis::cmd("EXPIRE")
            .arg(Self::key(user_id))
            .arg(HASH_TTL_SECONDS)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn reserve_redis(
        &self,
        user_id: Uuid,
        bytes: i64,
        estimated_tokens: i64,
        limits: IngestionLimits,
    ) -> Result<bool, String> {
        match self
            .run_reserve(user_id, bytes, estimated_tokens, limits)
            .await
        {
            Ok(value) => Ok(value),
            Err(_) => {
                self.rebuild_redis(user_id).await?;
                self.run_reserve(user_id, bytes, estimated_tokens, limits)
                    .await
            }
        }
    }

    pub async fn reserve(
        &self,
        user_id: Uuid,
        file_id: Uuid,
        bytes: i64,
        limits: IngestionLimits,
    ) -> Result<Option<Uuid>, String> {
        let bytes = bytes.max(1);
        // Conservative pre-reservation until the extractor reports actual tokens.
        let estimated_tokens = (bytes / 3).max(1);
        if !self
            .reserve_redis(user_id, bytes, estimated_tokens, limits)
            .await?
        {
            return Ok(None);
        }
        let result = async {
            let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
            sqlx::query("INSERT INTO ingestion_quota_usage(user_id) VALUES($1) ON CONFLICT(user_id) DO NOTHING")
                .bind(user_id).execute(&mut *tx).await.map_err(|error| error.to_string())?;
            let usage: UsageWindowRow = sqlx::query_as("SELECT hour_started_at,day_started_at,month_started_at,files_hour,files_day,bytes_day,tokens_month,pending_jobs FROM ingestion_quota_usage WHERE user_id=$1 FOR UPDATE")
                .bind(user_id).fetch_one(&mut *tx).await.map_err(|error| error.to_string())?;
            let now = Utc::now();
            let hour = if usage.0.date_naive() == now.date_naive() && usage.0.hour() == now.hour() { usage.3 } else { 0 };
            let day = if usage.1.date_naive() == now.date_naive() { usage.4 } else { 0 };
            let day_bytes = if usage.1.date_naive() == now.date_naive() { usage.5 } else { 0 };
            let month = if usage.2.year() == now.year() && usage.2.month() == now.month() { usage.6 } else { 0 };
            if (limits.files_per_hour > 0 && hour + 1 > limits.files_per_hour)
                || (limits.files_per_day > 0 && day + 1 > limits.files_per_day)
                || (limits.bytes_per_day > 0 && day_bytes + bytes > limits.bytes_per_day)
                || (limits.tokens_per_month > 0 && month + estimated_tokens > limits.tokens_per_month)
                || (limits.pending > 0 && usage.7 + 1 > limits.pending)
            {
                return Err("ingestion quota exceeded".to_owned());
            }
            let reservation_id = Uuid::now_v7();
            sqlx::query("UPDATE ingestion_quota_usage SET hour_started_at=date_trunc('hour',now()),day_started_at=date_trunc('day',now()),month_started_at=date_trunc('month',now()),files_hour=$2,files_day=$3,bytes_day=$4,tokens_month=$5,pending_jobs=pending_jobs+1,updated_at=now() WHERE user_id=$1")
                .bind(user_id).bind(hour + 1).bind(day + 1).bind(day_bytes + bytes).bind(month + estimated_tokens)
                .execute(&mut *tx).await.map_err(|error| error.to_string())?;
            sqlx::query("INSERT INTO ingestion_quota_reservations(id,user_id,file_id,status,reserved_bytes,estimated_tokens) VALUES($1,$2,$3,'reserved',$4,$5)")
                .bind(reservation_id).bind(user_id).bind(file_id).bind(bytes).bind(estimated_tokens)
                .execute(&mut *tx).await.map_err(|error| error.to_string())?;
            tx.commit().await.map_err(|error| error.to_string())?;
            Ok(reservation_id)
        }.await;
        match result {
            Ok(id) => Ok(Some(id)),
            Err(error) => {
                let _ = self
                    .release_redis(user_id, bytes, estimated_tokens, true, false)
                    .await;
                Err(error)
            }
        }
    }

    async fn release_redis(
        &self,
        user_id: Uuid,
        bytes: i64,
        tokens: i64,
        pending: bool,
        concurrent: bool,
    ) -> Result<(), String> {
        self.run_script(
            RELEASE_SCRIPT,
            user_id,
            &[1, bytes, tokens, i64::from(pending), i64::from(concurrent)],
        )
        .await
        .map(|_| ())
    }

    pub async fn begin(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        limits: IngestionLimits,
    ) -> Result<bool, String> {
        if !self
            .run_script(BEGIN_SCRIPT, user_id, &[limits.concurrent])
            .await?
        {
            return Ok(false);
        }
        let result = async {
            let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
            let changed = sqlx::query("UPDATE ingestion_quota_reservations SET status='processing',started_at=now() WHERE id=$1 AND user_id=$2 AND status='reserved'")
                .bind(reservation_id).bind(user_id).execute(&mut *tx).await.map_err(|error| error.to_string())?;
            if changed.rows_affected() != 1 {
                return Err("reservation is not available".to_owned());
            }
            sqlx::query("UPDATE ingestion_quota_usage SET pending_jobs=GREATEST(pending_jobs-1,0),concurrent_jobs=concurrent_jobs+1,updated_at=now() WHERE user_id=$1")
                .bind(user_id).execute(&mut *tx).await.map_err(|error| error.to_string())?;
            tx.commit().await.map_err(|error| error.to_string())?;
            Ok(())
        }.await;
        if let Err(error) = result {
            let _ = self.run_script(PAUSE_SCRIPT, user_id, &[]).await;
            if error == "reservation is not available" {
                return Ok(false);
            }
            return Err(error);
        }
        Ok(true)
    }

    /// Keeps the file/day/month reservation while returning a failed attempt to pending.
    pub async fn pause(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        error: &str,
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let changed = sqlx::query("UPDATE ingestion_quota_reservations SET status='reserved',last_error=$1,started_at=NULL WHERE id=$2 AND user_id=$3 AND status='processing'")
            .bind(error).bind(reservation_id).bind(user_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        if changed.rows_affected() == 1 {
            sqlx::query("UPDATE ingestion_quota_usage SET concurrent_jobs=GREATEST(concurrent_jobs-1,0),pending_jobs=pending_jobs+1,updated_at=now() WHERE user_id=$1")
                .bind(user_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        if changed.rows_affected() == 1
            && self.run_script(PAUSE_SCRIPT, user_id, &[]).await.is_err()
        {
            self.rebuild_redis(user_id).await?;
        }
        Ok(())
    }

    pub async fn finish(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        actual_tokens: i64,
        success: bool,
        error: Option<&str>,
    ) -> Result<(), String> {
        let row: Option<(i64, i64)> = sqlx::query_as("SELECT reserved_bytes,estimated_tokens FROM ingestion_quota_reservations WHERE id=$1 AND user_id=$2 AND status='processing'")
            .bind(reservation_id).bind(user_id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?;
        let Some((bytes, estimated)) = row else {
            return Ok(());
        };
        let actual_tokens = actual_tokens.max(0);
        if success {
            let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("UPDATE ingestion_quota_reservations SET status='completed',actual_tokens=$1,finished_at=now() WHERE id=$2 AND user_id=$3 AND status='processing'")
                .bind(actual_tokens).bind(reservation_id).bind(user_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            sqlx::query("UPDATE ingestion_quota_usage SET concurrent_jobs=GREATEST(concurrent_jobs-1,0),tokens_month=GREATEST(tokens_month+$2-$3,0),updated_at=now() WHERE user_id=$1")
                .bind(user_id).bind(actual_tokens).bind(estimated).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
            if self
                .run_script(RECONCILE_SCRIPT, user_id, &[actual_tokens, estimated])
                .await
                .is_err()
            {
                self.rebuild_redis(user_id).await?;
            }
        } else {
            self.release_reservation(user_id, reservation_id, error.unwrap_or("ingestion failed"))
                .await?;
        }
        let _ = bytes;
        Ok(())
    }

    /// Releases all counters. It is idempotent and safe to call when an outbox
    /// transaction or a terminal DLQ attempt fails after reserving capacity.
    pub async fn release_reservation(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        error: &str,
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let row: Option<(i64, i64, String)> = sqlx::query_as("SELECT reserved_bytes,estimated_tokens,status FROM ingestion_quota_reservations WHERE id=$1 AND user_id=$2 AND status IN ('reserved','processing') FOR UPDATE")
            .bind(reservation_id).bind(user_id).fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
        if let Some((bytes, estimated, status)) = row {
            sqlx::query("UPDATE ingestion_quota_reservations SET status='released',finished_at=now(),last_error=$1 WHERE id=$2 AND user_id=$3")
                .bind(error).bind(reservation_id).bind(user_id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            let pending = i64::from(status == "reserved");
            let concurrent = i64::from(status == "processing");
            sqlx::query("UPDATE ingestion_quota_usage SET files_hour=GREATEST(files_hour-1,0),files_day=GREATEST(files_day-1,0),bytes_day=GREATEST(bytes_day-$2,0),tokens_month=GREATEST(tokens_month-$3,0),pending_jobs=GREATEST(pending_jobs-$4,0),concurrent_jobs=GREATEST(concurrent_jobs-$5,0),updated_at=now() WHERE user_id=$1")
                .bind(user_id).bind(bytes).bind(estimated).bind(pending).bind(concurrent).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
            if self
                .release_redis(user_id, bytes, estimated, pending > 0, concurrent > 0)
                .await
                .is_err()
            {
                let _ = self.rebuild_redis(user_id).await;
            }
        } else {
            tx.commit().await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{IngestionLimits, QuotaManager};
    use sqlx::PgPool;
    use uuid::Uuid;

    #[tokio::test]
    #[ignore = "requires migrated FILE_SERVICE_DATABASE_URL and REDIS_URL services"]
    async fn concurrent_reservations_never_exceed_file_limit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database_url = std::env::var("FILE_SERVICE_DATABASE_URL")?;
        let redis_url = std::env::var("REDIS_URL")?;
        let pool = PgPool::connect(&database_url).await?;
        let redis = common::startup::connect_redis("file-service-test", &redis_url, 10, 5000).await?;
        let manager = QuotaManager::new(pool, redis);
        let user = Uuid::now_v7();
        let limits = IngestionLimits {
            files_per_hour: 0,
            files_per_day: 4,
            bytes_per_day: 1024,
            tokens_per_month: 0,
            concurrent: 4,
            pending: 4,
        };
        let mut tasks = Vec::new();
        for attempt in 0..32 {
            let manager = manager.clone();
            tasks.push(tokio::spawn(async move {
                let file = Uuid::new_v5(&user, format!("quota-race-{attempt}").as_bytes());
                sqlx::query("INSERT INTO files(id,user_id,name,mime_type,size_bytes,sha256,status,object_key,idempotency_key) VALUES($1,$2,'quota-race.txt','text/plain',1,$3,$4,$5,$6)")
                    .bind(file)
                    .bind(user)
                    .bind("0".repeat(64))
                    .bind("pending")
                    .bind(format!("tests/{file}"))
                    .bind(format!("quota-race-{file}"))
                    .execute(&manager.pool)
                    .await
                    .map_err(|error| error.to_string())?;
                manager.reserve(user, file, 1, limits).await
                    .map(|reservation| (file, reservation))
            }));
        }
        let mut reservations = Vec::new();
        let mut files = Vec::new();
        for task in tasks {
            let (file, reservation) = task.await??;
            files.push(file);
            if let Some(reservation) = reservation {
                reservations.push(reservation);
            }
        }
        if reservations.len() > 4 {
            return Err("concurrent quota reservations exceeded the plan limit".into());
        }
        for reservation in reservations {
            manager
                .release_reservation(user, reservation, "test cleanup")
                .await?;
        }
        sqlx::query(
            "DELETE FROM ingestion_quota_reservations WHERE user_id=$1 AND file_id=ANY($2)",
        )
        .bind(user)
        .bind(&files)
        .execute(&manager.pool)
        .await?;
        for file in files {
            sqlx::query("DELETE FROM files WHERE id=$1 AND user_id=$2")
                .bind(file)
                .bind(user)
                .execute(&manager.pool)
                .await?;
        }
        Ok(())
    }
}
