#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"
: "${REDIS_URL:?REDIS_URL is required}"
: "${NATS_URL:?NATS_URL is required}"

echo "checking postgres..."
if command -v psql >/dev/null 2>&1; then
  psql "${DATABASE_URL}" -v ON_ERROR_STOP=1 -Atc "SELECT current_database(), current_user, version();" >/dev/null
  echo "postgres ok"
else
  echo "psql not installed; skipping postgres check"
fi

echo "checking redis..."
if command -v redis-cli >/dev/null 2>&1; then
  redis-cli -u "${REDIS_URL}" ping >/dev/null
  echo "redis ok"
else
  echo "redis-cli not installed; skipping redis check"
fi

echo "checking nats..."
if command -v nats >/dev/null 2>&1; then
  nats --server "${NATS_URL}" server check connection >/dev/null
  echo "nats ok"
else
  echo "nats cli not installed; skipping nats check"
fi

if [[ -n "${QDRANT_URL:-}" ]]; then
  echo "checking qdrant..."
  if command -v curl >/dev/null 2>&1; then
    if [[ -n "${QDRANT_API_KEY:-}" ]]; then
      curl -fsS -H "api-key: ${QDRANT_API_KEY}" "${QDRANT_URL%/}/collections" >/dev/null
    else
      curl -fsS "${QDRANT_URL%/}/collections" >/dev/null
    fi
    echo "qdrant ok"
  else
    echo "curl not installed; skipping qdrant check"
  fi
fi

echo "runtime smoke check completed"
