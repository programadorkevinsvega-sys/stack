#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"

if [ -f "$ROOT_DIR/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$ROOT_DIR/.env"
  set +a
fi

if [ "${SECRETS_PROVIDER:-env}" = "vault" ]; then
  runtime_secrets_file="${SECRETS_RUNTIME_FILE:-$ROOT_DIR/.runtime/vault/runtime.env}"
  if [[ "$runtime_secrets_file" != /* ]]; then
    runtime_secrets_file="$ROOT_DIR/$runtime_secrets_file"
  fi
  if [ ! -s "$runtime_secrets_file" ]; then
    echo "Vault runtime secrets are not ready. Start Vault through ./run-all.sh first." >&2
    exit 1
  fi
  set -a
  # shellcheck disable=SC1090
  source "$runtime_secrets_file"
  set +a
fi

echo "========================================"
echo "  Building all workspace members"
echo "  Target: $CARGO_TARGET_DIR"
echo "========================================"

APPS=(
  api-gateway
  api-service
  auth-service
  billing-service
  credential-manager
  customer-bff
  file-service
  s3-gateway
  admin-bff
  invoice-service
  key-services
  model-router
  payment-services
  subscription-service
  usage-service
)

# Builds use the committed SQLx metadata. Database-backed validation is isolated
# in scripts/prepare-sqlx-offline.sh and is run only when SQL or migrations change.
echo ""
echo "--- cargo check (shared crates) ---"
SQLX_OFFLINE=true cargo check -p common -p db -p events -p proto -p bedrock-client

echo ""
echo "--- cargo check (apps, SQLx offline metadata) ---"
for app in "${APPS[@]}"; do
  echo "  Checking: $app"
  SQLX_OFFLINE=true cargo check -p "$app"
done

echo ""
echo "--- cargo build --release (apps) ---"
for app in "${APPS[@]}"; do
  echo "  Building: $app"
  SQLX_OFFLINE=true cargo build --release -p "$app"
done

# Build dashboard (if Node.js is available)
if command -v node &>/dev/null && [ -f "$ROOT_DIR/client-platform-dashboard/package.json" ]; then
  echo ""
  echo "--- Building client-platform-dashboard ---"
  cd "$ROOT_DIR/client-platform-dashboard"

  if command -v bun &>/dev/null; then
    bun install --frozen-lockfile 2>/dev/null || bun install
    bun run build
  elif command -v npm &>/dev/null; then
    npm ci 2>/dev/null || npm install
    npm run build
  fi

  cd "$ROOT_DIR"
fi

# Build the admin dashboard when its frontend workspace is present.
if command -v node &>/dev/null && [ -f "$ROOT_DIR/admin-plataform-dashboard/package.json" ]; then
  echo ""
  echo "--- Building admin-plataform-dashboard ---"
  cd "$ROOT_DIR/admin-plataform-dashboard"

  if command -v bun &>/dev/null; then
    bun install --frozen-lockfile 2>/dev/null || bun install
    bun run build
  elif command -v npm &>/dev/null; then
    npm ci 2>/dev/null || npm install
    npm run build
  fi

  cd "$ROOT_DIR"
fi

echo ""
echo "========================================"
echo "  Build complete!"
echo "========================================"
