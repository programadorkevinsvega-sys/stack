#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OFFLINE_DIR="$ROOT_DIR/.sqlx"

# shellcheck disable=SC1091
source "$ROOT_DIR/.env"

runtime_secret_json=""
if [[ "${SECRETS_PROVIDER:-env}" == "aws" ]]; then
  command -v aws >/dev/null 2>&1 || { echo "aws CLI is required when SECRETS_PROVIDER=aws" >&2; exit 1; }
  command -v jq >/dev/null 2>&1 || { echo "jq is required when SECRETS_PROVIDER=aws" >&2; exit 1; }
  runtime_secret_json="$(AWS_PROFILE="${AWS_PROFILE:-}" AWS_REGION="${AWS_REGION:-}" aws secretsmanager get-secret-value \
    --secret-id "${AWS_RUNTIME_SECRET_ID:-ai-gateway/${AWS_ENVIRONMENT:-dev}/runtime}" \
    --query SecretString --output text)"
fi

secret_value() {
  local name="$1"
  if [[ "${SECRETS_PROVIDER:-env}" == "aws" ]]; then
    jq -er --arg name "$name" '.[$name] // empty' <<<"$runtime_secret_json"
  else
    printf '%s' "${!name:-}"
  fi
}

declare -A APP_DATABASE_URL_VARS=(
  [api-service]=API_SERVICE_DATABASE_URL
  [auth-service]=AUTH_SERVICE_DATABASE_URL
  [billing-service]=BILLING_SERVICE_DATABASE_URL
  [credential-manager]=CREDENTIAL_MANAGER_DATABASE_URL
  [invoice-service]=INVOICE_SERVICE_DATABASE_URL
  [key-services]=KEY_SERVICES_DATABASE_URL
  [payment-services]=PAYMENT_SERVICES_DATABASE_URL
  [subscription-service]=SUBSCRIPTION_SERVICE_DATABASE_URL
  [usage-service]=USAGE_SERVICE_DATABASE_URL
)

mkdir -p "$OFFLINE_DIR"
find "$OFFLINE_DIR" -maxdepth 1 -type f -name 'query-*.json' -delete

for app in \
  api-service \
  auth-service \
  billing-service \
  credential-manager \
  invoice-service \
  key-services \
  payment-services \
  subscription-service \
  usage-service
do
  database_url_var="${APP_DATABASE_URL_VARS[$app]}"
  database_url="$(secret_value "$database_url_var")"
  if [ -z "$database_url" ]; then
    echo "$database_url_var is required to prepare SQLx metadata." >&2
    exit 1
  fi

  echo "Preparing SQLx metadata: $app"
  (
    cd "$ROOT_DIR"
    SQLX_OFFLINE=false \
      SQLX_OFFLINE_DIR="$OFFLINE_DIR" \
      DATABASE_URL="$database_url" \
      cargo check -p "$app"
  )
done

echo "SQLx offline metadata written to $OFFLINE_DIR"
