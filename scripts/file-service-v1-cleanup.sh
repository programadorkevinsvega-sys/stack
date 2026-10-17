#!/usr/bin/env bash
set -euo pipefail

DATABASE_URL="${FILE_SERVICE_DATABASE_URL:-${DATABASE_URL:-}}"
: "${DATABASE_URL:?FILE_SERVICE_DATABASE_URL or DATABASE_URL is required}"
command -v psql >/dev/null || { echo "psql is required to run the audited cleanup" >&2; exit 2; }
command -v pg_dump >/dev/null || { echo "pg_dump is required to create the rollback backup" >&2; exit 2; }
if [[ "${CONFIRM_FILE_SERVICE_V1_CLEANUP:-}" != "DELETE_V1_AFTER_BACKUP" ]]; then
  echo "Refusing cleanup. Set CONFIRM_FILE_SERVICE_V1_CLEANUP=DELETE_V1_AFTER_BACKUP after the stability window." >&2
  exit 2
fi
BACKUP_FILE="${FILE_SERVICE_V1_BACKUP_FILE:-}"
: "${BACKUP_FILE:?FILE_SERVICE_V1_BACKUP_FILE must point to a verified pg_dump file}"
[[ -s "$BACKUP_FILE" ]] || { echo "Backup file is missing or empty" >&2; exit 2; }

pg_dump "$DATABASE_URL" --format=custom --file="$BACKUP_FILE" --table=embedding_chunks --table=vector_migration_runs --table=vector_migration_reports
RUN_ID="$(cat /proc/sys/kernel/random/uuid)"
ACTOR="${FILE_SERVICE_MIGRATION_ACTOR:-operator}"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 \
  -v run_id="$RUN_ID" -v actor="$ACTOR" <<'SQL'
BEGIN;
INSERT INTO vector_migration_runs(id,operation,source_schema,target_schema,status,actor,details)
VALUES (:'run_id','cleanup','embedding_chunks_v1','embedding_vectors_v2','running',:'actor','{"backup_verified":true}'::jsonb);
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM vector_migration_reports WHERE cross_user_mismatches<>0 OR hash_mismatches<>0) THEN
    RAISE EXCEPTION 'migration reports contain mismatches; cleanup is blocked';
  END IF;
END $$;
DELETE FROM embedding_chunks;
UPDATE vector_migration_runs SET status='completed',completed_at=now() WHERE id=:'run_id';
COMMIT;
SQL
echo "legacy embedding_chunks cleanup completed under migration run $RUN_ID"
