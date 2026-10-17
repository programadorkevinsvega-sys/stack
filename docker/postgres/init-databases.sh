#!/bin/sh
set -eu

database_list="${POSTGRES_DATABASES:-gateway_db,router_db,credentials_db,auth_db,keys_db,catalog_db,subscription_db,payments_db,billing_db,usage_db,invoicing_db,customer_bff_db,files_db}"
database_owner="${POSTGRES_USER:-ai_gateway}"

if [ -n "${POSTGRES_PASSWORD:-}" ]; then
    psql --username "${database_owner}" --dbname postgres \
        --set ON_ERROR_STOP=1 \
        --set role_name="${database_owner}" \
        --set role_password="${POSTGRES_PASSWORD}" \
        >/dev/null <<'SQL'
ALTER ROLE :"role_name" WITH PASSWORD :'role_password';
SQL
fi

old_ifs="${IFS}"
IFS=','
for database_name in ${database_list}; do
    IFS="${old_ifs}"
    database_name="$(printf '%s' "${database_name}" | tr -d '[:space:]')"
    case "${database_name}" in
        ""|*[!a-z0-9_]*)
            echo "Invalid PostgreSQL database name: ${database_name}" >&2
            exit 1
            ;;
    esac

    if ! psql --username "${database_owner}" --dbname postgres --tuples-only \
        --command "SELECT 1 FROM pg_database WHERE datname = '${database_name}'" |
        grep -q 1; then
        createdb --username "${database_owner}" --owner "${database_owner}" "${database_name}"
        echo "Created PostgreSQL database: ${database_name}"
    fi

    psql --username "${database_owner}" --dbname "${database_name}" \
        --set ON_ERROR_STOP=1 \
        --command "CREATE EXTENSION IF NOT EXISTS citext; CREATE EXTENSION IF NOT EXISTS pgcrypto;" \
        >/dev/null
    IFS=','
done
IFS="${old_ifs}"
