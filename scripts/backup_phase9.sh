#!/usr/bin/env bash
set -euo pipefail

backup_dir="${BACKUP_DIR:?BACKUP_DIR is required}"
database_list="${BACKUP_DATABASES:?BACKUP_DATABASES must contain name=url pairs separated by spaces}"
mkdir -p "${backup_dir}"
command -v pg_dump >/dev/null || { printf '%s\n' "pg_dump is required" >&2; exit 2; }
command -v sha256sum >/dev/null || { printf '%s\n' "sha256sum is required" >&2; exit 2; }

IFS=' ' read -r -a database_entries <<<"${database_list}"
for database_entry in "${database_entries[@]}"; do
    database_name="${database_entry%%=*}"
    database_url="${database_entry#*=}"
    [[ -n "${database_name}" && "${database_entry}" == *=* ]] || { printf '%s\n' "BACKUP_DATABASES entries must be name=url" >&2; exit 2; }
    pg_dump --format=custom --no-owner --no-acl --file "${backup_dir}/${database_name}.dump" "${database_url}"
done

if [[ -n "${FILE_SERVICE_DATABASE_URL:-}" ]]; then
    command -v psql >/dev/null || { printf '%s\n' "psql is required for the R2 manifest" >&2; exit 2; }
    psql "${FILE_SERVICE_DATABASE_URL}" --no-psqlrc --tuples-only --csv \
        -c "SELECT id, user_id, object_key, size_bytes, content_hash, status FROM files ORDER BY id" \
        >"${backup_dir}/file-metadata.csv"
fi

if [[ -n "${R2_ENDPOINT:-}" && -n "${R2_ACCESS_KEY_ID:-}" && -n "${R2_SECRET_ACCESS_KEY:-}" && -n "${R2_BUCKET:-}" ]]; then
    command -v rclone >/dev/null || { printf '%s\n' "rclone is required for R2 backup" >&2; exit 2; }
    rclone_config="$(mktemp)"
    trap 'rm -f "${rclone_config}"' EXIT
    umask 077
    {
        printf '%s\n' '[r2]'
        printf '%s\n' 'type = s3'
        printf '%s\n' 'provider = Cloudflare'
        printf 'access_key_id = %s\n' "${R2_ACCESS_KEY_ID}"
        printf 'secret_access_key = %s\n' "${R2_SECRET_ACCESS_KEY}"
        printf 'endpoint = %s\n' "${R2_ENDPOINT}"
    } >"${rclone_config}"
    mkdir -p "${backup_dir}/r2-objects"
    RCLONE_CONFIG="${rclone_config}" rclone copy "r2:${R2_BUCKET}" "${backup_dir}/r2-objects" --checksum --transfers "${R2_BACKUP_TRANSFERS:-8}" --checkers "${R2_BACKUP_CHECKERS:-16}"
    RCLONE_CONFIG="${rclone_config}" rclone size "r2:${R2_BUCKET}" >"${backup_dir}/r2-size.txt"
fi

if [[ -n "${QDRANT_URL:-}" && -n "${QDRANT_COLLECTIONS:-}" ]]; then
    command -v curl >/dev/null || { printf '%s\n' "curl is required for Qdrant snapshots" >&2; exit 2; }
    command -v jq >/dev/null || { printf '%s\n' "jq is required for Qdrant snapshots" >&2; exit 2; }
    IFS=',' read -r -a collections <<<"${QDRANT_COLLECTIONS}"
    for collection in "${collections[@]}"; do
        collection="${collection//[[:space:]]/}"
        if [[ -n "${QDRANT_API_KEY:-}" ]]; then
            snapshot_json="$(curl --fail-with-body --silent --show-error -X POST -H "api-key: ${QDRANT_API_KEY}" "${QDRANT_URL%/}/collections/${collection}/snapshots")"
            snapshot_name="$(jq -er '.result.name' <<<"${snapshot_json}")"
            curl --fail-with-body --silent --show-error -H "api-key: ${QDRANT_API_KEY}" -o "${backup_dir}/qdrant-${collection}-${snapshot_name}" "${QDRANT_URL%/}/collections/${collection}/snapshots/${snapshot_name}"
        else
            snapshot_json="$(curl --fail-with-body --silent --show-error -X POST "${QDRANT_URL%/}/collections/${collection}/snapshots")"
            snapshot_name="$(jq -er '.result.name' <<<"${snapshot_json}")"
            curl --fail-with-body --silent --show-error -o "${backup_dir}/qdrant-${collection}-${snapshot_name}" "${QDRANT_URL%/}/collections/${collection}/snapshots/${snapshot_name}"
        fi
    done
fi

find "${backup_dir}" -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum >"${backup_dir}/SHA256SUMS"
printf '%s\n' "Backup completed: ${backup_dir}"
