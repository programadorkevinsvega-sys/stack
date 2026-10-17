#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="$project_root/artifacts/verify/fase8"
mkdir -p "$artifact_dir"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
log_file="$artifact_dir/$timestamp.log"
json_file="$artifact_dir/$timestamp.json"

if [[ -f "$project_root/.env" ]]; then
  set -a
  source "$project_root/.env"
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
  local name="$1"
  shift
  printf '\n[%s] %s\n' "$name" "$*" | tee -a "$log_file"
  if "$@" >>"$log_file" 2>&1; then
    printf '%s=passed\n' "$name" | tee -a "$log_file"
    return 0
  fi
  printf '%s=failed\n' "$name" | tee -a "$log_file"
  return 1
}

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
status=0
integration_status="skipped"
run_check file_service_tests cargo test -p file-service -- --nocapture || status=1
if [[ -n "${FILE_SERVICE_DATABASE_URL:-}" && -n "${REDIS_URL:-}" ]]; then
  integration_status="executed"
  run_check integration_security_and_race_tests cargo test -p file-service -- --ignored --nocapture || status=1
else
  printf '\nintegration_security_and_race_tests=skipped (FILE_SERVICE_DATABASE_URL and REDIS_URL unavailable)\n' | tee -a "$log_file"
fi
run_check clippy cargo clippy --workspace --all-targets --all-features -- -D warnings || status=1

finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
if [[ "$status" -eq 0 ]]; then
  result="passed"
else
  result="failed"
fi
cat >"$json_file" <<EOF
{
  "phase": "8",
  "result": "$result",
  "started_at": "$started_at",
  "finished_at": "$finished_at",
  "checks": ["file_service_tests", "integration_security_and_race_tests", "clippy"],
  "log": "$log_file",
  "real_dependency_tests": "$integration_status"
}
EOF
cp "$json_file" "$artifact_dir/latest.json"
exit "$status"
