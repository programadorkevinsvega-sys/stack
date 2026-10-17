#!/usr/bin/env bash
set -euo pipefail

DATABASE_URL="${FILE_SERVICE_DATABASE_URL:-${DATABASE_URL:-}}"
: "${DATABASE_URL:?FILE_SERVICE_DATABASE_URL or DATABASE_URL is required}"
command -v psql >/dev/null || { echo "psql is required to run the audited rollout" >&2; exit 2; }
ACTION="${1:-backfill}"
ACTOR="${FILE_SERVICE_MIGRATION_ACTOR:-operator}"
RUN_ID="$(cat /proc/sys/kernel/random/uuid)"

case "$ACTION" in
  backfill)
    SQL_FILE="scripts/file-service-v2-backfill.sql"
    ;;
  compare)
    SQL_FILE="scripts/file-service-v2-compare.sql"
    ;;
  *)
    echo "usage: $0 [backfill|compare]" >&2
    exit 2
    ;;
esac

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -v run_id="$RUN_ID" -v actor="$ACTOR" -f "$SQL_FILE"
echo "migration run $RUN_ID completed ($ACTION)"
