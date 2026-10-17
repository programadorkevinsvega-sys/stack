#!/usr/bin/env bash
set -euo pipefail

target_url="${LOAD_TEST_URL:-http://127.0.0.1:4000/health}"
request_count="${LOAD_TEST_REQUESTS:-100}"
parallelism="${LOAD_TEST_CONCURRENCY:-16}"
http_method="${LOAD_TEST_METHOD:-GET}"
request_body="${LOAD_TEST_BODY:-}"
content_type="${LOAD_TEST_CONTENT_TYPE:-application/json}"
result_file="${LOAD_TEST_OUTPUT:-}"

[[ "${request_count}" =~ ^[1-9][0-9]*$ ]] || { printf '%s\n' "LOAD_TEST_REQUESTS must be a positive integer" >&2; exit 2; }
[[ "${parallelism}" =~ ^[1-9][0-9]*$ ]] || { printf '%s\n' "LOAD_TEST_CONCURRENCY must be a positive integer" >&2; exit 2; }
command -v curl >/dev/null || { printf '%s\n' "curl is required" >&2; exit 2; }

temporary_dir="$(mktemp -d)"
cleanup() { rm -rf "${temporary_dir}"; }
trap cleanup EXIT

request_one() {
    request_id="$1"
    response_file="${temporary_dir}/${request_id}.response"
    timing_file="${temporary_dir}/${request_id}.timing"
    curl_args=(--silent --show-error --output "${response_file}" --write-out '%{http_code} %{time_total}' --request "${http_method}" "${target_url}")
    if [[ -n "${request_body}" ]]; then
        curl_args+=(--header "Content-Type: ${content_type}" --data-binary "${request_body}")
    fi
    if [[ -n "${LOAD_TEST_AUTH_HEADER:-}" ]]; then
        curl_args+=(--header "${LOAD_TEST_AUTH_HEADER}")
    fi
    if [[ -n "${LOAD_TEST_EXTRA_HEADER:-}" ]]; then
        curl_args+=(--header "${LOAD_TEST_EXTRA_HEADER}")
    fi
    if response_line="$(curl "${curl_args[@]}" 2>"${temporary_dir}/${request_id}.error")"; then
        printf '%s\n' "${response_line}" >"${timing_file}"
    else
        printf '%s\n' "000 0" >"${timing_file}"
    fi
}
export -f request_one
export temporary_dir target_url http_method request_body content_type
export LOAD_TEST_AUTH_HEADER="${LOAD_TEST_AUTH_HEADER:-}"
export LOAD_TEST_EXTRA_HEADER="${LOAD_TEST_EXTRA_HEADER:-}"

seq 1 "${request_count}" | xargs -r -n1 -P "${parallelism}" bash -c 'request_one "$1"' bash

summary="$(awk '
    BEGIN { success = 0; failure = 0 }
    $1 ~ /^[0-9]+$/ { total += 1; if ($1 >= 200 && $1 < 400) success += 1; else failure += 1; latency[total] = $2 }
    END {
      if (total == 0) { print "0 0 0 0 0 0"; exit }
      for (i = 1; i <= total; i++) sorted[i] = latency[i]
      for (i = 1; i <= total; i++) for (j = i + 1; j <= total; j++) if (sorted[j] < sorted[i]) { tmp = sorted[i]; sorted[i] = sorted[j]; sorted[j] = tmp }
      p50 = sorted[int((total * 50 + 99) / 100)]; p95 = sorted[int((total * 95 + 99) / 100)]; p99 = sorted[int((total * 99 + 99) / 100)]
      print total, success, failure, p50, p95, p99
    }
' "${temporary_dir}"/*.timing)"
read -r total success failure p50 p95 p99 <<<"${summary}"
report="{\"url\":\"${target_url//\\/\\\\}\",\"requests\":${total},\"success\":${success},\"failure\":${failure},\"p50_seconds\":${p50},\"p95_seconds\":${p95},\"p99_seconds\":${p99},\"concurrency\":${parallelism}}"
if [[ -n "${result_file}" ]]; then
    printf '%s\n' "${report}" >"${result_file}"
fi
printf '%s\n' "${report}"
(( failure == 0 ))
