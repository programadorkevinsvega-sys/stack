use events::outbox::{enqueue_in_transaction, OutboxStore};
use events::{EventEnvelope, UsageRecorded, EVENT_USAGE_RECORDED};
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn test_table() -> (PgPool, String) {
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("failed to connect to test postgres");
    let table = format!("event_outbox_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE {table} (
            id UUID PRIMARY KEY,
            event_type VARCHAR(100) NOT NULL,
            payload JSONB NOT NULL,
            status VARCHAR(20) NOT NULL,
            attempts INT NOT NULL,
            available_at TIMESTAMPTZ NOT NULL,
            locked_at TIMESTAMPTZ,
            last_error TEXT,
            created_at TIMESTAMPTZ NOT NULL,
            published_at TIMESTAMPTZ
        )"
    ))
    .execute(&pool)
    .await
    .expect("failed to create isolated outbox table");
    (pool, table)
}

fn usage_envelope() -> EventEnvelope<UsageRecorded> {
    EventEnvelope::new(
        EVENT_USAGE_RECORDED,
        UsageRecorded {
            request_id: Uuid::now_v7(),
            api_key_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            model_catalog_id: Uuid::new_v4(),
            aws_account_id: Uuid::new_v4(),
            input_tokens: 10,
            output_tokens: 20,
            cached_tokens: 0,
            stream: false,
            latency_ms: 50,
            ttft_ms: None,
            status_code: 200,
            error_code: None,
        },
    )
}

async fn drop_test_table(pool: &PgPool, table: &str) {
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(pool)
        .await
        .expect("failed to remove isolated test table");
}

#[tokio::test]
async fn standalone_enqueue_creates_pending_row() {
    let (pool, table) = test_table().await;
    let store = OutboxStore::new(pool.clone(), &table).unwrap();
    let envelope = usage_envelope();

    let outbox_id = store
        .enqueue(EVENT_USAGE_RECORDED, &envelope)
        .await
        .expect("enqueue should succeed");
    let row = sqlx::query(&format!(
        "SELECT event_type,status,attempts,payload FROM {table} WHERE id=$1"
    ))
    .bind(outbox_id)
    .fetch_one(&pool)
    .await
    .expect("outbox row should exist");

    assert_eq!(row.get::<String, _>("event_type"), EVENT_USAGE_RECORDED);
    assert_eq!(row.get::<String, _>("status"), "pending");
    assert_eq!(row.get::<i32, _>("attempts"), 0);
    assert_eq!(
        row.get::<serde_json::Value, _>("payload")["event_id"],
        envelope.event_id.to_string()
    );
    drop_test_table(&pool, &table).await;
}

#[tokio::test]
async fn transactional_enqueue_rolls_back_with_business_transaction() {
    let (pool, table) = test_table().await;
    let envelope = usage_envelope();
    let mut tx = pool.begin().await.unwrap();
    enqueue_in_transaction(&mut tx, &table, EVENT_USAGE_RECORDED, &envelope)
        .await
        .expect("transactional enqueue should succeed");
    tx.rollback().await.unwrap();

    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    drop_test_table(&pool, &table).await;
}

#[tokio::test]
async fn transactional_enqueue_commits_with_business_transaction() {
    let (pool, table) = test_table().await;
    let envelope = usage_envelope();
    let mut tx = pool.begin().await.unwrap();
    enqueue_in_transaction(&mut tx, &table, EVENT_USAGE_RECORDED, &envelope)
        .await
        .expect("transactional enqueue should succeed");
    tx.commit().await.unwrap();

    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    drop_test_table(&pool, &table).await;
}
