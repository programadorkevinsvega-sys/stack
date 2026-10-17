#!/usr/bin/env bash
set -euo pipefail

[[ "${RESTORE_CONFIRM:-}" == "RESTORE" ]] || { printf '%s\n' "Set RESTORE_CONFIRM=RESTORE to enable destructive restore" >&2; exit 2; }
restore_dir="${RESTORE_DIR:?RESTORE_DIR is required}"
database_list="${RESTORE_DATABASES:?RESTORE_DATABASES must contain name=url pairs separated by spaces}"
[[ -d "${restore_dir}" ]] || { printf '%s\n' "RESTORE_DIR does not exist" >&2; exit 2; }
command -v pg_restore >/dev/null || { printf '%s\n' "pg_restore is required" >&2; exit 2; }
if [[ -f "${restore_dir}/SHA256SUMS" ]]; then
    (cd "${restore_dir}" && sha256sum --check SHA256SUMS)
fi

IFS=' ' read -r -a database_entries <<<"${database_list}"
for database_entry in "${database_entries[@]}"; do
    database_name="${database_entry%%=*}"
    database_url="${database_entry#*=}"
    pg_restore --exit-on-error --single-transaction --clean --if-exists --no-owner --no-acl \
        --dbname "${database_url}" "${restore_dir}/${database_name}.dump"
done

if [[ -n "${R2_ENDPOINT:-}" && -n "${R2_ACCESS_KEY_ID:-}" && -n "${R2_SECRET_ACCESS_KEY:-}" && -n "${R2_BUCKET:-}" && -d "${restore_dir}/r2-objects" ]]; then
    command -v rclone >/dev/null || { printf '%s\n' "rclone is required for R2 restore" >&2; exit 2; }
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
    RCLONE_CONFIG="${rclone_config}" rclone copy "${restore_dir}/r2-objects" "r2:${R2_BUCKET}" --checksum --transfers "${R2_BACKUP_TRANSFERS:-8}" --checkers "${R2_BACKUP_CHECKERS:-16}"
fi

printf '%s\n' "Metadata and R2 restore completed. Restore Qdrant snapshots separately after validating the target collection."
