#!/usr/bin/env bash
# ------------------------------------------------------------------
# run-all.sh — Start backend microservices from production binaries
#
# Usage:
#   ./run-all.sh          # start all backend services
#   ./run-all.sh --no-follow # start without streaming combined logs
#   ./run-all.sh --stop   # stop backend processes started by this script
#
# PostgreSQL, Redis, NATS and Qdrant must already be running. Runtime secrets
# are loaded by each binary from AWS Secrets Manager; this script never reads
# Vault and never injects a Vault fallback.
# This script never starts containers or frontend applications.
# ------------------------------------------------------------------
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOGS_DIR="$ROOT_DIR/target/logs"
PID_FILE="$ROOT_DIR/target/.run-all-pids"
CLEANUP_DONE=false
LOG_TAIL_PID=""
declare -A SERVICE_PIDS=()

mkdir -p "$LOGS_DIR"

# Read the environment contract for orchestration-only tasks (migrations and
# health checks). These variables stay shell-local; each service independently
# loads .env and AWS configuration through common::AppConfig.
if [ -f "$ROOT_DIR/.env" ]; then
  # shellcheck disable=SC1091
  source "$ROOT_DIR/.env"
fi

# ─── helpers ───────────────────────────────────────────────
cleanup() {
  if [ "$CLEANUP_DONE" = true ]; then
    return
  fi
  CLEANUP_DONE=true

  echo ""
  echo "Stopping backend services..."
  if [ -n "$LOG_TAIL_PID" ]; then
    kill "$LOG_TAIL_PID" 2>/dev/null || true
  fi
  if [ -f "$PID_FILE" ]; then
    service_pids=()
    while IFS= read -r pid; do
      service_pids+=("$pid")
      kill "$pid" 2>/dev/null || true
    done < "$PID_FILE"
    for _ in $(seq 1 20); do
      running=false
      for pid in "${service_pids[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
          running=true
          break
        fi
      done
      if [ "$running" = false ]; then
        break
      fi
      sleep 0.1
    done
    for pid in "${service_pids[@]}"; do
      if kill -0 "$pid" 2>/dev/null; then
        kill -KILL "$pid" 2>/dev/null || true
      fi
    done
    rm -f "$PID_FILE"
  fi
  echo "Done."
}

start_rust_service() {
  local name=$1
  local package=$2
  local log=$LOGS_DIR/$name.log

  echo "  Starting: $name  (log: $log)"
  (
    service_binary="$ROOT_DIR/target/release/$package"
    if [ ! -x "$service_binary" ]; then
      echo "Production binary not found: $service_binary" >&2
      echo "Run ./build-all.sh before starting the backend." >&2
      exit 1
    fi
    exec "$service_binary"
  ) >"$log" 2>&1 &
  local pid=$!
  SERVICE_PIDS["$package"]=$pid
  echo "$pid" >> "$PID_FILE"
}

wait_for_healthy() {
  local name=$1
  local max=$2
  local command=$3
  echo -n "  Waiting for $name"
  for attempt in $(seq 1 "$max"); do
    if eval "$command" 2>/dev/null; then
      echo "  [OK]"
      return 0
    fi
    echo -n "."
    sleep 2
  done
  echo "  [TIMEOUT]"
  return 1
}

wait_for_log_ready() {
  local name=$1
  local max=$2
  local package=$3
  local pid="${SERVICE_PIDS[$package]:-}"
  local log="$LOGS_DIR/$package.log"
  echo -n "  Waiting for $name"
  for i in $(seq 1 "$max"); do
    if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
      echo "  [FAILED]"
      echo "  $name stopped before becoming healthy." >&2
      if [ -f "$log" ]; then
        echo "  Last log entries from $log:" >&2
        tail -n 40 "$log" >&2
      fi
      return 1
    fi
    if [ -f "$log" ] && grep -Eq '"message":"[^"]*listening' "$log"; then
      echo "  [OK]"
      return 0
    fi
    echo -n "."
    sleep 2
  done
  echo "  [TIMEOUT]"
  return 1
}

# ─── parse args ────────────────────────────────────────────
FOLLOW_LOGS=true

for arg in "$@"; do
  case "$arg" in
    --build)
      echo "run-all.sh only starts production binaries; use ./build-all.sh first." >&2
      exit 1
      ;;
    --no-follow) FOLLOW_LOGS=false ;;
    --no-dashboard) ;; # Kept as a backwards-compatible no-op; frontend never starts.
    --stop) cleanup; exit 0 ;;
    *) echo "Unknown option: $arg"; exit 1 ;;
  esac
done

# ─── stop previous ─────────────────────────────────────────
if [ -f "$PID_FILE" ]; then
  echo "Previous session detected. Stopping first..."
  cleanup
  CLEANUP_DONE=false
fi

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ─── AWS runtime secret for migrations ─────────────────────
AWS_RUNTIME_SECRET_JSON=""
if [ "${SECRETS_PROVIDER:-env}" = "aws" ]; then
  if ! command -v aws >/dev/null 2>&1 || ! command -v jq >/dev/null 2>&1; then
    echo "SECRETS_PROVIDER=aws requires both aws CLI and jq for SQLx migrations." >&2
    exit 1
  fi
  AWS_RUNTIME_SECRET_JSON="$(AWS_PROFILE="${AWS_PROFILE:-}" AWS_REGION="${AWS_REGION:-}" aws secretsmanager get-secret-value \
    --secret-id "${AWS_RUNTIME_SECRET_ID:-ai-gateway/${AWS_ENVIRONMENT:-dev}/runtime}" \
    --query SecretString --output text)"
  if [ -z "$AWS_RUNTIME_SECRET_JSON" ] || [ "$AWS_RUNTIME_SECRET_JSON" = "None" ]; then
    echo "AWS runtime secret is empty; refusing to start without Secrets Manager." >&2
    exit 1
  fi
fi

secret_value() {
  local name="$1"
  if [ "${SECRETS_PROVIDER:-env}" = "aws" ]; then
    jq -er --arg name "$name" '.[$name] // empty' <<<"$AWS_RUNTIME_SECRET_JSON"
  else
    printf '%s' "${!name:-}"
  fi
}

# ─── backend dependency checks ─────────────────────────────
echo ""
echo "========================================"
echo "  Checking backend dependencies"
echo "========================================"

dependency_error() {
  echo "" >&2
  echo "A required backend dependency is unavailable." >&2
  echo "Start PostgreSQL, Redis, NATS and Qdrant separately, then retry." >&2
}

wait_for_healthy "PostgreSQL" 1 "bash -c '</dev/tcp/127.0.0.1/5432'" || {
  dependency_error
  exit 1
}
wait_for_healthy "Redis" 1 "bash -c '</dev/tcp/127.0.0.1/6379'" || {
  dependency_error
  exit 1
}
wait_for_healthy "NATS" 1 "bash -c '</dev/tcp/127.0.0.1/4222'" || {
  dependency_error
  exit 1
}
wait_for_healthy "Qdrant" 1 "bash -c '</dev/tcp/127.0.0.1/6333'" || {
  dependency_error
  exit 1
}
# ─── database migrations ───────────────────────────────────
echo ""
echo "========================================"
echo "  Running SQLx migrations"
echo "========================================"

# The shared PostgreSQL container creates databases only on first initialization.
# Ensure the new bounded context also works with an already-existing volume.
base_database_url="${DATABASE_URL:-}"
if [ "${SECRETS_PROVIDER:-env}" = "aws" ]; then
  base_database_url="$(secret_value DATABASE_URL)"
fi
if [ -n "$base_database_url" ] && command -v psql >/dev/null 2>&1; then
  if ! psql "$base_database_url" -Atqc "SELECT 1 FROM pg_database WHERE datname='files_db'" | grep -q 1; then
    echo "  Creating PostgreSQL database: files_db"
    psql "$base_database_url" -v ON_ERROR_STOP=1 -c 'CREATE DATABASE files_db' >/dev/null
  fi
fi

APPS_WITH_MIGRATIONS=(
  api-gateway
  model-router
  credential-manager
  auth-service
  key-services
  api-service
  subscription-service
  payment-services
  billing-service
  usage-service
  invoice-service
  customer-bff
  file-service
  admin-bff
)

declare -A APP_DATABASE_URL_VARS=(
  [api-gateway]=API_GATEWAY_DATABASE_URL
  [model-router]=MODEL_ROUTER_DATABASE_URL
  [credential-manager]=CREDENTIAL_MANAGER_DATABASE_URL
  [auth-service]=AUTH_SERVICE_DATABASE_URL
  [key-services]=KEY_SERVICES_DATABASE_URL
  [api-service]=API_SERVICE_DATABASE_URL
  [subscription-service]=SUBSCRIPTION_SERVICE_DATABASE_URL
  [payment-services]=PAYMENT_SERVICES_DATABASE_URL
  [billing-service]=BILLING_SERVICE_DATABASE_URL
  [usage-service]=USAGE_SERVICE_DATABASE_URL
  [invoice-service]=INVOICE_SERVICE_DATABASE_URL
  [customer-bff]=CUSTOMER_BFF_DATABASE_URL
  [file-service]=FILE_SERVICE_DATABASE_URL
)

if command -v sqlx >/dev/null 2>&1; then
  for app in "${APPS_WITH_MIGRATIONS[@]}"; do
    mig_dir="$ROOT_DIR/apps/$app/migrations"
    if [ -d "$mig_dir" ] && [ -n "$(ls -A "$mig_dir" 2>/dev/null)" ]; then
      echo "  Migrations: $app"
      database_url_var="${APP_DATABASE_URL_VARS[$app]}"
      migration_database_url="$(secret_value "$database_url_var")"
      if [ "$app" = "file-service" ] && [ -z "$migration_database_url" ] && [ -n "$base_database_url" ]; then
        migration_database_url="${base_database_url%/*}/files_db"
      fi
      if [ -z "$migration_database_url" ]; then
        echo "$database_url_var is required for $app migrations." >&2
        exit 1
      fi

      (
        cd "$ROOT_DIR"
        DATABASE_URL="$migration_database_url" \
          sqlx migrate run --no-dotenv --source "apps/$app/migrations"
      )
    fi
  done
else
  echo "  Migrations skipped: sqlx-cli is not installed."
fi

# ─── microservices ─────────────────────────────────────────
echo ""
echo "========================================"
echo "  Starting microservices"
echo "========================================"

# Order matters: services with no deps on other services first.
# Each runs in the project root so it picks up .env and crate paths.
cd "$ROOT_DIR"

# Catalog is an eager dependency of billing-service.
start_rust_service "api-service" "api-service"
wait_for_log_ready "api-service gRPC" 120 "api-service"

# Independent services start in parallel.
start_rust_service "credential-manager" "credential-manager"
start_rust_service "auth-service" "auth-service"
start_rust_service "key-services" "key-services"
start_rust_service "subscription-service" "subscription-service"
start_rust_service "payment-services" "payment-services"
start_rust_service "usage-service" "usage-service"
start_rust_service "invoice-service" "invoice-service"
start_rust_service "file-service" "file-service"

wait_for_log_ready "credential-manager gRPC" 120 "credential-manager"
wait_for_log_ready "auth-service gRPC" 120 "auth-service"
wait_for_log_ready "key-services gRPC" 120 "key-services"
wait_for_log_ready "subscription-service gRPC" 120 "subscription-service"
wait_for_log_ready "payment-services gRPC" 120 "payment-services"
wait_for_log_ready "usage-service gRPC" 120 "usage-service"
wait_for_log_ready "invoice-service gRPC" 120 "invoice-service"
wait_for_log_ready "file-service gRPC" 120 "file-service"

start_rust_service "model-router" "model-router"
start_rust_service "billing-service" "billing-service"
wait_for_log_ready "model-router gRPC" 120 "model-router"
wait_for_log_ready "billing-service gRPC" 120 "billing-service"

start_rust_service "api-gateway" "api-gateway"
start_rust_service "customer-bff" "customer-bff"
start_rust_service "admin-bff" "admin-bff"
start_rust_service "s3-gateway" "s3-gateway"
wait_for_log_ready "api-gateway HTTP" 120 "api-gateway"
wait_for_log_ready "customer-bff HTTP" 120 "customer-bff"
wait_for_log_ready "admin-bff HTTP" 120 "admin-bff"
wait_for_log_ready "s3-gateway HTTP" 120 "s3-gateway"

# ─── ready ─────────────────────────────────────────────────
echo ""
echo "========================================"
echo "  Backend services are running — logs in:"
echo "    $LOGS_DIR"
echo ""
if [ "$FOLLOW_LOGS" = true ]; then
  echo "  Streaming combined backend logs below."
  echo "  Every entry is also retained in its service log file."
else
  echo "  Combined log streaming disabled (--no-follow)."
  echo "  Follow one service with: tail -f $LOGS_DIR/api-gateway.log"
fi
echo ""
echo "  Stop all:"
echo "    $0 --stop"
echo "========================================"

# Keep script alive so trap fires on Ctrl+C
echo "Press Ctrl+C to stop all services."
if [ "$FOLLOW_LOGS" = true ]; then
  tail -n +1 -F "$LOGS_DIR"/*.log &
  LOG_TAIL_PID=$!
fi
wait
