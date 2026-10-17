#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${NATS_URL:?NATS_URL is required}"
: "${USER_ID:?USER_ID is required}"
: "${PAYMENT_TRANSACTION_ID:?PAYMENT_TRANSACTION_ID is required}"
: "${PROVIDER_REFERENCE:?PROVIDER_REFERENCE is required}"
: "${REQUEST_ID:?REQUEST_ID is required}"
: "${MODEL_CATALOG_ID:?MODEL_CATALOG_ID is required}"

psql "${DATABASE_URL}" -v ON_ERROR_STOP=1 <<SQL
INSERT INTO billing.wallets (user_id, currency)
VALUES ('${USER_ID}', 'USD')
ON CONFLICT (user_id) DO NOTHING;
SQL

nats pub usage.recorded "{
  \"event_id\":\"$(uuidgen)\",
  \"event_type\":\"usage.recorded\",
  \"schema_version\":1,
  \"occurred_at\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
  \"payload\":{
    \"request_id\":\"${REQUEST_ID}\",
    \"api_key_id\":\"$(uuidgen)\",
    \"user_id\":\"${USER_ID}\",
    \"model_catalog_id\":\"${MODEL_CATALOG_ID}\",
    \"aws_account_id\":\"$(uuidgen)\",
    \"input_tokens\":100,
    \"output_tokens\":100,
    \"cached_tokens\":0,
    \"stream\":false,
    \"latency_ms\":100,
    \"ttft_ms\":null,
    \"status_code\":200,
    \"error_code\":null
  }
}"
