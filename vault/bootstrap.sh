#!/bin/sh
set -eu

RUNTIME_DIR="${VAULT_RUNTIME_DIR:-/vault/runtime}"
PKI_MOUNT="${VAULT_PKI_MOUNT:-pki-internal}"
PKI_ROLE="${VAULT_PKI_ROLE:-nexus-service-mtls}"
APPROLE_NAME="${VAULT_APPROLE_NAME:-nexus-services}"
POLICY_NAME="${VAULT_POLICY_NAME:-nexus-service}"
POLICY_FILE="${VAULT_POLICY_FILE:-/vault/policies/nexus-service.hcl}"
PUBLIC_ADDR="${VAULT_PUBLIC_ADDR:-http://localhost:8200}"
RUNTIME_UID="${VAULT_RUNTIME_UID:-1000}"
RUNTIME_GID="${VAULT_RUNTIME_GID:-1000}"
KV_MOUNT="${VAULT_KV_MOUNT:-secret}"
RUNTIME_SECRET_PATH="${VAULT_RUNTIME_SECRET_PATH:-dev/nexus/runtime}"
SECRET_KEYS_FILE="${VAULT_SECRET_KEYS_FILE:-/vault/bootstrap/runtime-secret-keys.txt}"
LOCAL_DATABASES_ENABLED="${LOCAL_DATABASES_ENABLED:-false}"
POSTGRES_HOST="${POSTGRES_HOST:-127.0.0.1}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_USER="${POSTGRES_USER:-ai_gateway}"
NATS_USERNAME="${NATS_USERNAME:-nats_admin}"

UNSEAL_KEY_FILE="${RUNTIME_DIR}/unseal-key"
ROOT_TOKEN_FILE="${RUNTIME_DIR}/root-token"
ROLE_ID_FILE="${RUNTIME_DIR}/role-id"
SECRET_ID_FILE="${RUNTIME_DIR}/secret-id"
SEED_FILE="${RUNTIME_DIR}/seed.env"
RUNTIME_ENV_FILE="${RUNTIME_DIR}/runtime.env"
READY_FILE="${RUNTIME_DIR}/ready"

mkdir -p "${RUNTIME_DIR}"
umask 077

write_private_file() {
    destination="$1"
    value="$2"
    temporary="${destination}.tmp.$$"
    printf '%s\n' "${value}" > "${temporary}"
    chmod 0600 "${temporary}"
    mv "${temporary}" "${destination}"
}

write_application_file() {
    destination="$1"
    value="$2"
    temporary="${destination}.tmp.$$"
    printf '%s\n' "${value}" > "${temporary}"
    chmod 0600 "${temporary}"
    chown "${RUNTIME_UID}:${RUNTIME_GID}" "${temporary}"
    mv "${temporary}" "${destination}"
}

put_runtime_secret() {
    key="$1"
    value="$2"
    secret_path="${KV_MOUNT}/${RUNTIME_SECRET_PATH}/${key}"
    value_file="$(mktemp /tmp/nexus-vault-runtime.XXXXXX)"
    chmod 0600 "${value_file}"
    printf '%s' "${value}" > "${value_file}"
    if ! vault kv put "${secret_path}" "value=@${value_file}" >/dev/null; then
        rm -f "${value_file}"
        return 1
    fi
    rm -f "${value_file}"
}

read_runtime_secret() {
    vault kv get -field=value \
        "${KV_MOUNT}/${RUNTIME_SECRET_PATH}/$1" 2>/dev/null || true
}

ensure_random_runtime_secret() {
    key="$1"
    value="$(read_runtime_secret "${key}")"
    if [ -z "${value}" ]; then
        value="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')"
        put_runtime_secret "${key}" "${value}"
    fi
    printf '%s' "${value}"
}

wait_for_vault() {
    until vault status -format=json >/dev/null 2>&1 || [ "$?" -eq 2 ]; do
        echo "Waiting for Vault API at ${VAULT_ADDR}..."
        sleep 2
    done
}

status_json() {
    vault status -format=json 2>/dev/null || true
}

initialize_if_needed() {
    status="$(status_json)"
    if printf '%s' "${status}" | grep -q '"initialized"[[:space:]]*:[[:space:]]*true'; then
        return
    fi

    echo "Initializing Vault with one local development unseal key..."
    initialization="$(vault operator init -key-shares=1 -key-threshold=1)"
    unseal_key="$(printf '%s\n' "${initialization}" | awk -F': ' '/^Unseal Key 1:/ { print $2 }')"
    root_token="$(printf '%s\n' "${initialization}" | awk -F': ' '/^Initial Root Token:/ { print $2 }')"

    if [ -z "${unseal_key}" ] || [ -z "${root_token}" ]; then
        echo "Vault initialization did not return the expected credentials." >&2
        exit 1
    fi

    write_private_file "${UNSEAL_KEY_FILE}" "${unseal_key}"
    write_private_file "${ROOT_TOKEN_FILE}" "${root_token}"
    rm -f "${ROLE_ID_FILE}" "${SECRET_ID_FILE}" "${READY_FILE}"
}

unseal_if_needed() {
    status="$(status_json)"
    if ! printf '%s' "${status}" | grep -q '"sealed"[[:space:]]*:[[:space:]]*true'; then
        return
    fi

    if [ ! -s "${UNSEAL_KEY_FILE}" ]; then
        echo "Vault is sealed but ${UNSEAL_KEY_FILE} is missing." >&2
        exit 1
    fi

    echo "Unsealing Vault..."
    vault operator unseal "$(cat "${UNSEAL_KEY_FILE}")" >/dev/null
}

configure_pki() {
    if ! vault secrets list | grep -q "^${PKI_MOUNT}/"; then
        echo "Enabling PKI secrets engine at ${PKI_MOUNT}/..."
        vault secrets enable -path="${PKI_MOUNT}" pki >/dev/null
    fi

    vault secrets tune -max-lease-ttl=87600h "${PKI_MOUNT}" >/dev/null

    if ! vault read "${PKI_MOUNT}/cert/ca" >/dev/null 2>&1; then
        echo "Generating local development root CA..."
        vault write "${PKI_MOUNT}/root/generate/internal" \
            common_name="Nexus Local Development Root CA" \
            key_type=rsa \
            key_bits=4096 \
            ttl=87600h >/dev/null
    fi

    vault write "${PKI_MOUNT}/config/urls" \
        issuing_certificates="${PUBLIC_ADDR}/v1/${PKI_MOUNT}/ca" \
        crl_distribution_points="${PUBLIC_ADDR}/v1/${PKI_MOUNT}/crl" >/dev/null

    allowed_domains="svc.internal,api-gateway,auth-service,api-service,credential-manager,key-services,model-router,subscription-service,payment-services,billing-service,usage-service,invoice-service,customer-bff,admin-bff,file-service,s3-gateway"
    vault write "${PKI_MOUNT}/roles/${PKI_ROLE}" \
        allowed_domains="${allowed_domains}" \
        allow_bare_domains=true \
        allow_subdomains=true \
        allow_localhost=true \
        allow_ip_sans=true \
        enforce_hostnames=true \
        server_flag=true \
        client_flag=true \
        key_type=rsa \
        key_bits=2048 \
        max_ttl=72h >/dev/null
}

configure_runtime_secrets() {
    if ! vault secrets list | grep -q "^${KV_MOUNT}/"; then
        echo "Enabling KV v2 secrets engine at ${KV_MOUNT}/..."
        vault secrets enable -path="${KV_MOUNT}" kv-v2 >/dev/null
    fi

    if [ -s "${SEED_FILE}" ]; then
        echo "Importing critical runtime values into Vault KV..."
        set -a
        # The seed is generated from the local shell-compatible .env file.
        # shellcheck disable=SC1090
        . "${SEED_FILE}"
        set +a

        while IFS= read -r key || [ -n "${key}" ]; do
            case "${key}" in
                ""|\#*) continue ;;
            esac
            value="$(printenv "${key}" 2>/dev/null || true)"
            if [ -z "${value}" ]; then
                continue
            fi
            secret_path="${KV_MOUNT}/${RUNTIME_SECRET_PATH}/${key}"
            if ! vault kv get "${secret_path}" >/dev/null 2>&1; then
                value_file="$(mktemp /tmp/nexus-vault-import.XXXXXX)"
                chmod 0600 "${value_file}"
                printf '%s' "${value}" > "${value_file}"
                if ! vault kv put "${secret_path}" "value=@${value_file}" >/dev/null; then
                    rm -f "${value_file}"
                    return 1
                fi
                rm -f "${value_file}"
            fi
            vault kv get "${secret_path}" >/dev/null
        done < "${SECRET_KEYS_FILE}"

        : > "${SEED_FILE}"
        chmod 0600 "${SEED_FILE}"
        chown "${RUNTIME_UID}:${RUNTIME_GID}" "${SEED_FILE}"

        while IFS= read -r key || [ -n "${key}" ]; do
            case "${key}" in
                ""|\#*) continue ;;
            esac
            unset "${key}"
        done < "${SECRET_KEYS_FILE}"
    fi

    if [ "${LOCAL_DATABASES_ENABLED}" = "true" ]; then
        postgres_password="$(ensure_random_runtime_secret POSTGRES_PASSWORD)"

        for internal_key in \
            API_KEY_PEPPER \
            JWT_SIGNING_SECRET \
            SERVICE_JWT_SECRET \
            AUDIT_HMAC_SECRET \
            RESEND_API_KEY \
            PAYPAL_CLIENT_ID \
            PAYPAL_CLIENT_SECRET \
            PAYPAL_WEBHOOK_ID
        do
            ensure_random_runtime_secret "${internal_key}" >/dev/null
        done

        postgres_base_url="postgres://${POSTGRES_USER}:${postgres_password}@${POSTGRES_HOST}:${POSTGRES_PORT}"
        put_runtime_secret DATABASE_URL "${postgres_base_url}/postgres"

        for mapping in \
            "API_GATEWAY_DATABASE_URL=gateway_db" \
            "MODEL_ROUTER_DATABASE_URL=router_db" \
            "CREDENTIAL_MANAGER_DATABASE_URL=credentials_db" \
            "AUTH_SERVICE_DATABASE_URL=auth_db" \
            "KEY_SERVICES_DATABASE_URL=keys_db" \
            "API_SERVICE_DATABASE_URL=catalog_db" \
            "SUBSCRIPTION_SERVICE_DATABASE_URL=subscription_db" \
            "PAYMENT_SERVICES_DATABASE_URL=payments_db" \
            "BILLING_SERVICE_DATABASE_URL=billing_db" \
            "USAGE_SERVICE_DATABASE_URL=usage_db" \
            "INVOICE_SERVICE_DATABASE_URL=invoicing_db" \
            "CUSTOMER_BFF_DATABASE_URL=customer_bff_db" \
            "FILE_SERVICE_DATABASE_URL=files_db"
        do
            key="${mapping%%=*}"
            database_name="${mapping#*=}"
            put_runtime_secret "${key}" "${postgres_base_url}/${database_name}"
        done

        redis_password="$(ensure_random_runtime_secret REDIS_PASSWORD)"
        put_runtime_secret REDIS_URL \
            "redis://:${redis_password}@127.0.0.1:6379/5"

        nats_password="$(ensure_random_runtime_secret NATS_PASSWORD)"
        put_runtime_secret NATS_URL \
            "nats://${NATS_USERNAME}:${nats_password}@127.0.0.1:4222"

        ensure_random_runtime_secret PGADMIN_DEFAULT_PASSWORD >/dev/null
        unset postgres_password postgres_base_url redis_password nats_password
    fi

    # Metrics pseudonymization is required in every deployment mode, including
    # external PostgreSQL/Redis environments where LOCAL_DATABASES_ENABLED is false.
    ensure_random_runtime_secret OBSERVABILITY_USER_HASH_SALT >/dev/null
}

render_runtime_secrets() {
    temporary="${RUNTIME_ENV_FILE}.tmp.$$"
    : > "${temporary}"

    while IFS= read -r key || [ -n "${key}" ]; do
        case "${key}" in
            ""|\#*) continue ;;
        esac
        secret_path="${KV_MOUNT}/${RUNTIME_SECRET_PATH}/${key}"
        if value="$(vault kv get -field=value "${secret_path}" 2>/dev/null)"; then
            escaped="$(printf '%s' "${value}" | sed "s/'/'\"'\"'/g")"
            printf "%s='%s'\n" "${key}" "${escaped}" >> "${temporary}"
        fi
    done < "${SECRET_KEYS_FILE}"

    chmod 0600 "${temporary}"
    chown "${RUNTIME_UID}:${RUNTIME_GID}" "${temporary}"
    mv "${temporary}" "${RUNTIME_ENV_FILE}"
}

configure_approle() {
    if ! vault auth list | grep -q '^approle/'; then
        echo "Enabling AppRole authentication..."
        vault auth enable approle >/dev/null
    fi

    vault policy write "${POLICY_NAME}" "${POLICY_FILE}" >/dev/null
    vault write "auth/approle/role/${APPROLE_NAME}" \
        token_policies="${POLICY_NAME}" \
        token_type=service \
        token_ttl=1h \
        token_max_ttl=4h \
        secret_id_ttl=0 \
        secret_id_num_uses=0 >/dev/null

    role_id="$(vault read -field=role_id "auth/approle/role/${APPROLE_NAME}/role-id")"
    stored_role_id=""
    if [ -s "${ROLE_ID_FILE}" ]; then
        stored_role_id="$(cat "${ROLE_ID_FILE}")"
    fi

    if [ "${role_id}" != "${stored_role_id}" ] || [ ! -s "${SECRET_ID_FILE}" ]; then
        secret_id="$(vault write -field=secret_id -f "auth/approle/role/${APPROLE_NAME}/secret-id")"
        write_application_file "${ROLE_ID_FILE}" "${role_id}"
        write_application_file "${SECRET_ID_FILE}" "${secret_id}"
    fi
}

verify_application_access() {
    role_id="$(cat "${ROLE_ID_FILE}")"
    secret_id="$(cat "${SECRET_ID_FILE}")"
    application_token="$(
        vault write -field=token "auth/approle/login" \
            role_id="${role_id}" \
            secret_id="${secret_id}"
    )"

    VAULT_TOKEN="${application_token}" vault write -field=serial_number \
        "${PKI_MOUNT}/issue/${PKI_ROLE}" \
        common_name="api-gateway.svc.internal" \
        alt_names="api-gateway" \
        ttl=5m \
        private_key_format=pkcs8 >/dev/null

    while IFS= read -r key || [ -n "${key}" ]; do
        case "${key}" in
            ""|\#*) continue ;;
        esac
        secret_path="${KV_MOUNT}/${RUNTIME_SECRET_PATH}/${key}"
        if vault kv get "${secret_path}" >/dev/null 2>&1; then
            VAULT_TOKEN="${application_token}" vault kv get "${secret_path}" >/dev/null
            break
        fi
    done < "${SECRET_KEYS_FILE}"
}

bootstrap() {
    rm -f "${READY_FILE}"
    wait_for_vault
    initialize_if_needed
    unseal_if_needed

    if [ ! -s "${ROOT_TOKEN_FILE}" ]; then
        echo "Vault is initialized but ${ROOT_TOKEN_FILE} is missing." >&2
        exit 1
    fi

    VAULT_TOKEN="$(cat "${ROOT_TOKEN_FILE}")"
    export VAULT_TOKEN

    configure_pki
    configure_runtime_secrets
    render_runtime_secrets
    configure_approle
    verify_application_access
    printf 'ready\n' > "${READY_FILE}"
    chmod 0644 "${READY_FILE}"
    echo "Vault PKI and AppRole are ready."
}

bootstrap

while true; do
    if ! vault status >/dev/null 2>&1; then
        bootstrap
    fi
    sleep 5
done
