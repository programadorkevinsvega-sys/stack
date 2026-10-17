#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="${project_root}/artifacts/verify/fase9"
mkdir -p "${artifact_dir}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
log_file="${artifact_dir}/${timestamp}.log"
json_file="${artifact_dir}/${timestamp}.json"

if [[ -f "${project_root}/.env" ]]; then
    set -a
    source "${project_root}/.env"
    set +a
fi
if [[ "${SECRETS_PROVIDER:-env}" == "aws" ]]; then
    command -v aws >/dev/null 2>&1 || { echo "aws CLI is required when SECRETS_PROVIDER=aws" >&2; exit 1; }
    command -v jq >/dev/null 2>&1 || { echo "jq is required when SECRETS_PROVIDER=aws" >&2; exit 1; }
    runtime_secret_json="$(AWS_PROFILE="${AWS_PROFILE:-}" AWS_REGION="${AWS_REGION:-}" aws secretsmanager get-secret-value --secret-id "${AWS_RUNTIME_SECRET_ID:-ai-gateway/${AWS_ENVIRONMENT:-dev}/runtime}" --query SecretString --output text)"
    while IFS='=' read -r name value; do
        export "$name=$value"
    done < <(jq -r 'to_entries[] | select(.value|type == "string") | [.key,.value] | @tsv' <<<"$runtime_secret_json" | tr '\t' '=')
fi

run_check() {
    check_name="$1"
    shift
    printf '\n[%s] %s\n' "${check_name}" "$*" | tee -a "${log_file}"
    if "$@" >>"${log_file}" 2>&1; then
        printf '%s=passed\n' "${check_name}" | tee -a "${log_file}"
        return 0
    fi
    printf '%s=failed\n' "${check_name}" | tee -a "${log_file}"
    return 1
}

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
status=0
load_status="skipped"
recovery_status="skipped"
payment_status="skipped"

run_check shell_syntax bash -n "${project_root}/scripts/phase9_load_test.sh" || status=1
run_check shell_syntax_backup bash -n "${project_root}/scripts/backup_phase9.sh" || status=1
run_check shell_syntax_restore bash -n "${project_root}/scripts/restore_phase9.sh" || status=1
run_check shell_syntax_recovery bash -n "${project_root}/scripts/qdrant_recovery_phase9.sh" || status=1
run_check file_service_tests cargo test -p file-service || status=1
run_check cargo_check cargo check --workspace || status=1
run_check clippy cargo clippy --workspace --all-targets --all-features -- -D warnings || status=1
run_check compose_config docker compose config --quiet || status=1

if [[ "${PHASE9_RUN_LOAD_TEST:-false}" == "true" ]]; then
    load_status="executed"
    run_check load_test env LOAD_TEST_OUTPUT="${artifact_dir}/${timestamp}.load.json" "${project_root}/scripts/phase9_load_test.sh" || status=1
else
    printf '%s\n' 'load_test=skipped (set PHASE9_RUN_LOAD_TEST=true to hit a live endpoint)' | tee -a "${log_file}"
fi

if [[ "${PHASE9_RUN_RECOVERY_PLAN:-false}" == "true" && -n "${FILE_SERVICE_DATABASE_URL:-}" && "$(command -v psql || true)" != "" ]]; then
    recovery_status="executed"
    run_check recovery_manifest "${project_root}/scripts/qdrant_recovery_phase9.sh" || status=1
else
    printf '%s\n' 'recovery_manifest=skipped (set PHASE9_RUN_RECOVERY_PLAN=true with FILE_SERVICE_DATABASE_URL)' | tee -a "${log_file}"
fi

if [[ -n "${SUBSCRIPTION_SERVICE_DATABASE_URL:-}" && "$(command -v psql || true)" != "" ]]; then
    payment_status="executed"
    run_check payment_lifecycle_smoke "${project_root}/scripts/payment_lifecycle_smoke_phase9.sh" || status=1
else
    printf '%s\n' 'payment_lifecycle_smoke=skipped (SUBSCRIPTION_SERVICE_DATABASE_URL unavailable)' | tee -a "${log_file}"
fi

finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
result="failed"
if [[ "${status}" -eq 0 ]]; then
    result="passed"
fi
cat >"${json_file}" <<EOF
{
  "phase": "9",
  "result": "${result}",
  "started_at": "${started_at}",
  "finished_at": "${finished_at}",
  "checks": ["shell_syntax", "file_service_tests", "cargo_check", "clippy", "compose_config", "load_test", "recovery_manifest", "payment_lifecycle_smoke"],
  "load_test": "${load_status}",
  "recovery_manifest": "${recovery_status}",
  "payment_lifecycle_smoke": "${payment_status}",
  "log": "${log_file}"
}
EOF
cp "${json_file}" "${artifact_dir}/latest.json"
exit "${status}"
