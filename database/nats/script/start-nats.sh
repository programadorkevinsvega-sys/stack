#!/bin/sh
set -eu

bool_true() {
  value="$(printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]')"
  [ "$value" = "1" ] || [ "$value" = "true" ] || [ "$value" = "yes" ] || [ "$value" = "on" ]
}

require_file() {
  path="$1"
  label="$2"
  if [ ! -f "$path" ]; then
    echo "[nats-entrypoint] missing ${label}: ${path}" >&2
    exit 1
  fi
}

NATS_SERVER_NAME="${NATS_SERVER_NAME:-nats-jetstream-primary}"
NATS_PORT="${NATS_PORT:-4223}"
NATS_HTTP_PORT="${NATS_HTTP_PORT:-8223}"
NATS_CLUSTER_ENABLED="${NATS_CLUSTER_ENABLED:-false}"
NATS_CLUSTER_NAME="${NATS_CLUSTER_NAME:-nats-jetstream-cluster}"
NATS_CLUSTER_PORT="${NATS_CLUSTER_PORT:-6223}"
NATS_CLUSTER_LISTEN_HOST="${NATS_CLUSTER_LISTEN_HOST:-0.0.0.0}"
NATS_CLUSTER_ADVERTISE_HOST="${NATS_CLUSTER_ADVERTISE_HOST:-}"
NATS_CLUSTER_ADVERTISE_PORT="${NATS_CLUSTER_ADVERTISE_PORT:-$NATS_CLUSTER_PORT}"
NATS_CLUSTER_USERNAME="${NATS_CLUSTER_USERNAME:-}"
NATS_CLUSTER_PASSWORD="${NATS_CLUSTER_PASSWORD:-}"
NATS_CLUSTER_ROUTES="${NATS_CLUSTER_ROUTES:-}"
NATS_CORE_PLATFORM_USERNAME="${NATS_CORE_PLATFORM_USERNAME:-core_platform}"
NATS_CORE_PLATFORM_PASSWORD="${NATS_CORE_PLATFORM_PASSWORD:-}"
NATS_USERNAME="${NATS_USERNAME:-}"
NATS_PASSWORD="${NATS_PASSWORD:-}"
NATS_TLS_ENABLED="${NATS_TLS_ENABLED:-false}"
NATS_TLS_CERT_FILE="${NATS_TLS_CERT_FILE:-/etc/nats/ssl/server.crt}"
NATS_TLS_KEY_FILE="${NATS_TLS_KEY_FILE:-/etc/nats/ssl/server.key}"
NATS_TLS_CA_FILE="${NATS_TLS_CA_FILE:-/etc/nats/ssl/root.crt}"
NATS_MAX_PAYLOAD="${NATS_MAX_PAYLOAD:-16MB}"
NATS_MAX_PENDING="${NATS_MAX_PENDING:-256MB}"
NATS_WRITE_DEADLINE="${NATS_WRITE_DEADLINE:-10s}"
NATS_PING_INTERVAL="${NATS_PING_INTERVAL:-20s}"
NATS_PING_MAX="${NATS_PING_MAX:-3}"
NATS_MAX_CONNECTIONS="${NATS_MAX_CONNECTIONS:-20000}"
NATS_MAX_SUBSCRIPTIONS="${NATS_MAX_SUBSCRIPTIONS:-100000}"
NATS_JETSTREAM_STORE_DIR="${NATS_JETSTREAM_STORE_DIR:-/data/nats}"
NATS_JETSTREAM_MAX_MEMORY_STORE="${NATS_JETSTREAM_MAX_MEMORY_STORE:-1GB}"
NATS_JETSTREAM_MAX_FILE_STORE="${NATS_JETSTREAM_MAX_FILE_STORE:-250GB}"
NATS_JETSTREAM_SYNC_INTERVAL="${NATS_JETSTREAM_SYNC_INTERVAL:-2m}"

if [ -z "$NATS_USERNAME" ]; then
  echo "[nats-entrypoint] NATS_USERNAME is required" >&2
  exit 1
fi

if [ -z "$NATS_PASSWORD" ]; then
  echo "[nats-entrypoint] NATS_PASSWORD is required" >&2
  exit 1
fi

TLS_BLOCK=""
CLUSTER_TLS_BLOCK=""
if bool_true "$NATS_TLS_ENABLED"; then
  require_file "$NATS_TLS_CERT_FILE" "NATS TLS cert"
  require_file "$NATS_TLS_KEY_FILE" "NATS TLS key"
  TLS_BLOCK="
tls {
  cert_file: \"$NATS_TLS_CERT_FILE\"
  key_file: \"$NATS_TLS_KEY_FILE\"
"
  if [ -n "${NATS_TLS_CA_FILE:-}" ] && [ -f "$NATS_TLS_CA_FILE" ]; then
    TLS_BLOCK="${TLS_BLOCK}  ca_file: \"$NATS_TLS_CA_FILE\"
"
  fi
  TLS_BLOCK="${TLS_BLOCK}}"
  CLUSTER_TLS_BLOCK="
  tls {
    cert_file: \"$NATS_TLS_CERT_FILE\"
    key_file: \"$NATS_TLS_KEY_FILE\"
"
  if [ -n "${NATS_TLS_CA_FILE:-}" ] && [ -f "$NATS_TLS_CA_FILE" ]; then
    CLUSTER_TLS_BLOCK="${CLUSTER_TLS_BLOCK}    ca_file: \"$NATS_TLS_CA_FILE\"
    verify: true
"
  fi
  CLUSTER_TLS_BLOCK="${CLUSTER_TLS_BLOCK}  }"
fi

CLUSTER_BLOCK=""
if bool_true "$NATS_CLUSTER_ENABLED"; then
  if [ -z "$NATS_CLUSTER_USERNAME" ] || [ -z "$NATS_CLUSTER_PASSWORD" ]; then
    echo "[nats-entrypoint] NATS_CLUSTER_USERNAME and NATS_CLUSTER_PASSWORD are required when NATS_CLUSTER_ENABLED=true" >&2
    exit 1
  fi

  ROUTES_BLOCK=""
  if [ -n "$NATS_CLUSTER_ROUTES" ]; then
    OLD_IFS="${IFS}"
    IFS=','
    set -- $NATS_CLUSTER_ROUTES
    IFS="${OLD_IFS}"
    for ROUTE in "$@"; do
      CLEAN_ROUTE="$(printf '%s' "$ROUTE" | sed 's/^ *//;s/ *$//')"
      if [ -n "$CLEAN_ROUTE" ]; then
        ROUTES_BLOCK="${ROUTES_BLOCK}    \"${CLEAN_ROUTE}\",
"
      fi
    done
  fi

  CLUSTER_BLOCK="
cluster {
  name: \"${NATS_CLUSTER_NAME}\"
  listen: \"${NATS_CLUSTER_LISTEN_HOST}:${NATS_CLUSTER_PORT}\"
  authorization {
    user: \"${NATS_CLUSTER_USERNAME}\"
    password: \"${NATS_CLUSTER_PASSWORD}\"
  }
"
  if [ -n "$NATS_CLUSTER_ADVERTISE_HOST" ]; then
    CLUSTER_BLOCK="${CLUSTER_BLOCK}  advertise: \"${NATS_CLUSTER_ADVERTISE_HOST}:${NATS_CLUSTER_ADVERTISE_PORT}\"
"
  fi
  if [ -n "$ROUTES_BLOCK" ]; then
    CLUSTER_BLOCK="${CLUSTER_BLOCK}  routes = [
${ROUTES_BLOCK}  ]
"
  fi
  if [ -n "$CLUSTER_TLS_BLOCK" ]; then
    CLUSTER_BLOCK="${CLUSTER_BLOCK}${CLUSTER_TLS_BLOCK}
"
  fi
  CLUSTER_BLOCK="${CLUSTER_BLOCK}}"
fi

AUTH_BLOCK=""
if [ -n "$NATS_CORE_PLATFORM_PASSWORD" ]; then
  AUTH_BLOCK="authorization {
  users = [
    {
      user: \"${NATS_USERNAME}\"
      password: \"${NATS_PASSWORD}\"
    },
    {
      user: \"${NATS_CORE_PLATFORM_USERNAME}\"
      password: \"${NATS_CORE_PLATFORM_PASSWORD}\"
    }
  ]
}"
else
  AUTH_BLOCK="authorization {
  user: \"${NATS_USERNAME}\"
  password: \"${NATS_PASSWORD}\"
}"
fi

cat > /tmp/nats-server.generated.conf <<EOF
server_name: ${NATS_SERVER_NAME}
port: ${NATS_PORT}
http: 0.0.0.0:${NATS_HTTP_PORT}

max_payload: ${NATS_MAX_PAYLOAD}
max_pending: ${NATS_MAX_PENDING}
write_deadline: "${NATS_WRITE_DEADLINE}"
ping_interval: "${NATS_PING_INTERVAL}"
ping_max: ${NATS_PING_MAX}
disable_sublist_cache: false

${AUTH_BLOCK}

jetstream {
  store_dir: "${NATS_JETSTREAM_STORE_DIR}"
  max_memory_store: ${NATS_JETSTREAM_MAX_MEMORY_STORE}
  max_file_store: ${NATS_JETSTREAM_MAX_FILE_STORE}
  sync_interval: "${NATS_JETSTREAM_SYNC_INTERVAL}"
}

max_connections: ${NATS_MAX_CONNECTIONS}
max_subscriptions: ${NATS_MAX_SUBSCRIPTIONS}

debug: false
trace: false
logtime: true
${TLS_BLOCK}
${CLUSTER_BLOCK}
EOF

exec nats-server -c /tmp/nats-server.generated.conf
