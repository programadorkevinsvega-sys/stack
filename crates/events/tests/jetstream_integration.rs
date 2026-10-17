use async_nats::jetstream;
use async_nats::HeaderMap;
use events::jetstream::{
    ack, dead_letter, ensure_streams, pull_consumer_config, DeadLetter, DLQ_STREAM, EVENTS_STREAM,
};
use events::outbox::{OutboxConfig, OutboxRelay, OutboxStore};
use events::{EventEnvelope, UsageRecorded, EVENT_USAGE_RECORDED};
use futures_util::StreamExt;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

async fn next_message(consumer: &jetstream::consumer::PullConsumer) -> jetstream::Message {
    let mut messages = consumer
        .batch()
        .max_messages(1)
        .expires(Duration::from_secs(5))
        .messages()
        .await
        .expect("failed to request JetStream batch");
    tokio::time::timeout(Duration::from_secs(6), messages.next())
        .await
        .expect("timed out waiting for JetStream message")
        .expect("JetStream batch ended before a message arrived")
        .expect("JetStream returned a message error")
}

#[tokio::test]
#[ignore = "requires PostgreSQL and NATS JetStream; run with NATS_URL set"]
async fn relays_outbox_and_dead_letters_invalid_event() {
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let nats_url = std::env::var("NATS_URL").expect("NATS_URL must be set");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("failed to connect to test postgres");
    let client = common::connect_nats(&nats_url)
        .await
        .expect("failed to connect to test NATS");
    let context = jetstream::new(client);
    ensure_streams(&context)
        .await
        .expect("failed to configure test streams");

    let suffix = Uuid::new_v4().simple().to_string();
    let table = format!("event_outbox_js_test_{suffix}");
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

    let envelope = EventEnvelope::new(
        EVENT_USAGE_RECORDED,
        UsageRecorded {
            request_id: Uuid::now_v7(),
            api_key_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            model_catalog_id: Uuid::new_v4(),
            aws_account_id: Uuid::new_v4(),
            input_tokens: 1,
            output_tokens: 2,
            cached_tokens: 0,
            stream: false,
            latency_ms: 10,
            ttft_ms: None,
            status_code: 200,
            error_code: None,
        },
    );
    let outbox = OutboxStore::new(pool.clone(), &table).unwrap();
    let outbox_id = outbox
        .enqueue(EVENT_USAGE_RECORDED, &envelope)
        .await
        .expect("failed to enqueue test event");
    let relay = OutboxRelay::new(
        pool.clone(),
        context.clone(),
        OutboxConfig::for_service(format!("test_{suffix}"), &table),
    )
    .unwrap();
    assert_eq!(relay.relay_batch().await.unwrap(), 1);

    let status: String = sqlx::query_scalar(&format!("SELECT status FROM {table} WHERE id=$1"))
        .bind(outbox_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "published");

    let consumer_name = format!("test_{suffix}");
    let stream = context.get_stream(EVENTS_STREAM).await.unwrap();
    let consumer = stream
        .create_consumer(pull_consumer_config(&consumer_name, EVENT_USAGE_RECORDED, 10).unwrap())
        .await
        .unwrap();
    let message = next_message(&consumer).await;
    let received: EventEnvelope<UsageRecorded> =
        EventEnvelope::from_json(&message.payload).unwrap();
    assert_eq!(received.event_id, envelope.event_id);
    ack(&message).await.unwrap();

    let invalid_message_id = format!("invalid_{suffix}");
    let mut headers = HeaderMap::new();
    headers.insert("Nats-Msg-Id", invalid_message_id);
    context
        .publish_with_headers(
            EVENT_USAGE_RECORDED.to_string(),
            headers,
            b"not-json".as_slice().into(),
        )
        .await
        .unwrap()
        .await
        .unwrap();
    let invalid = next_message(&consumer).await;
    dead_letter(
        &context,
        &invalid,
        &consumer_name,
        "invalid integration-test payload",
    )
    .await
    .unwrap();

    let dlq_subject = format!("dlq.{consumer_name}.{EVENT_USAGE_RECORDED}");
    let dlq_consumer_name = format!("dlq_test_{suffix}");
    let dlq_stream = context.get_stream(DLQ_STREAM).await.unwrap();
    let dlq_consumer = dlq_stream
        .create_consumer(pull_consumer_config(&dlq_consumer_name, &dlq_subject, 10).unwrap())
        .await
        .unwrap();
    let dlq_message = next_message(&dlq_consumer).await;
    let dead_letter: DeadLetter = serde_json::from_slice(&dlq_message.payload).unwrap();
    assert_eq!(dead_letter.consumer, consumer_name);
    assert_eq!(dead_letter.original_subject, EVENT_USAGE_RECORDED);
    assert_eq!(dead_letter.payload, "6e6f742d6a736f6e");
    ack(&dlq_message).await.unwrap();

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .expect("failed to remove isolated test table");
}
