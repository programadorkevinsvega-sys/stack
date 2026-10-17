#!/usr/bin/env bash
set -euo pipefail

recovery_dir="${RECOVERY_DIR:-artifacts/recovery/qdrant}"
mkdir -p "${recovery_dir}"
database_url="${FILE_SERVICE_DATABASE_URL:?FILE_SERVICE_DATABASE_URL is required}"
command -v psql >/dev/null || { printf '%s\n' "psql is required" >&2; exit 2; }
psql "${database_url}" --no-psqlrc --tuples-only --csv \
    -c "SELECT id, user_id, object_key, mime_type, size_bytes, status FROM files WHERE status='ready' ORDER BY user_id,id" \
    >"${recovery_dir}/ready-files.csv"

if [[ "${RECOVERY_MODE:-plan}" == "execute" ]]; then
    : "${RECOVERY_CONFIRM:?Set RECOVERY_CONFIRM=REBUILD_QDRANT to dispatch rebuilds}"
    [[ "${RECOVERY_CONFIRM}" == "REBUILD_QDRANT" ]] || { printf '%s\n' "Invalid recovery confirmation" >&2; exit 2; }
    : "${FILE_SERVICE_REINDEX_URL:?FILE_SERVICE_REINDEX_URL is required for execute mode}"
    : "${RECOVERY_AUTH_HEADER:?RECOVERY_AUTH_HEADER is required for execute mode}"
    : "${RECOVERY_USER_ID:?RECOVERY_USER_ID is required; execute one authenticated owner at a time}"
    command -v curl >/dev/null || { printf '%s\n' "curl is required" >&2; exit 2; }
    while IFS=',' read -r file_id user_id object_key mime_type size_bytes status; do
        [[ "${file_id}" == "id" || -z "${file_id}" ]] && continue
        [[ "${user_id}" == "${RECOVERY_USER_ID}" ]] || continue
        curl --fail-with-body --silent --show-error -X POST \
            -H "Authorization: ${RECOVERY_AUTH_HEADER}" \
            -H 'Content-Type: application/json' \
            -d '{}' "${FILE_SERVICE_REINDEX_URL%/}/${file_id}/vectors/reindex" \
            >>"${recovery_dir}/dispatch.log"
        printf '\n' >>"${recovery_dir}/dispatch.log"
    done <"${recovery_dir}/ready-files.csv"
fi

printf '%s\n' "Qdrant recovery manifest created: ${recovery_dir}/ready-files.csv"
