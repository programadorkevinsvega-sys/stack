#[cfg(test)]
mod tests {
    use crate::domain::entities::UsageEvent;
    use crate::infrastructure::persistence::PgUsageRepository;
    use chrono::Utc;
    use sqlx::PgPool;
    use std::sync::Arc;
    use uuid::Uuid;

    async fn pool() -> PgPool {
        dotenv::dotenv().ok();
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");
        let pool = PgPool::connect(&db_url)
            .await
            .expect("failed to connect to test database");
        let migration = include_str!("../../migrations/20260718000001_init_usage.sql");
        for statement in migration.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Err(e) = sqlx::query(trimmed).execute(&pool).await {
                let msg = e.to_string();
                if msg.contains("already exists") || msg.contains("42P07") {
                    continue;
                }
                panic!("failed to run migration statement: {e}");
            }
        }
        pool
    }

    fn make_event(request_id: Uuid) -> UsageEvent {
        let now = Utc::now();
        UsageEvent {
            id: Uuid::now_v7(),
            request_id,
            api_key_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            model_catalog_id: Uuid::new_v4(),
            aws_account_id: Uuid::new_v4(),
            input_tokens: 150,
            output_tokens: 300,
            cached_tokens: 0,
            stream: false,
            latency_ms: 450,
            ttft_ms: None,
            status_code: 200,
            error_code: None,
            created_at: now,
        }
    }

    #[tokio::test]
    async fn save_event_is_idempotent_by_request_id() {
        let pool = pool().await;
        let repo = PgUsageRepository { pool };

        let request_id = Uuid::now_v7();
        let event1 = make_event(request_id);
        let event2 = make_event(request_id);

        let result1 = repo
            .save_event(&event1)
            .await
            .expect("first save should succeed");
        assert!(result1, "first save should return true (new event)");

        let result2 = repo
            .save_event(&event2)
            .await
            .expect("second save should succeed");
        assert!(
            !result2,
            "second save with same request_id should return false (duplicate)"
        );
    }

    #[tokio::test]
    async fn concurrent_save_event_enforces_idempotency() {
        let pool = pool().await;
        let repo = Arc::new(PgUsageRepository { pool });

        let request_id = Uuid::now_v7();
        let event1 = make_event(request_id);
        let event2 = make_event(request_id);

        let repo1 = Arc::clone(&repo);
        let repo2 = Arc::clone(&repo);

        let (r1, r2) = tokio::join!(
            tokio::spawn(async move { repo1.save_event(&event1).await }),
            tokio::spawn(async move { repo2.save_event(&event2).await }),
        );

        let r1 = r1.expect("task 1 panic").expect("task 1 save failed");
        let r2 = r2.expect("task 2 panic").expect("task 2 save failed");

        assert!(
            r1 != r2,
            "one concurrent call must succeed (true) and the other must detect duplicate (false), got r1={r1} r2={r2}"
        );
    }
}
