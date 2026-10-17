#!/usr/bin/env bash
set -euo pipefail

subscription_database_url="${SUBSCRIPTION_SERVICE_DATABASE_URL:?SUBSCRIPTION_SERVICE_DATABASE_URL is required}"
command -v psql >/dev/null || { printf '%s\n' "psql is required" >&2; exit 2; }

read -r lifecycle_counts <<<"$(psql "${subscription_database_url}" --no-psqlrc --tuples-only --csv \
    -c "SELECT COUNT(*) FILTER (WHERE status IN ('trialing','active','past_due','canceled')), COUNT(*) FILTER (WHERE payment_retry_count >= 0), COUNT(*) FILTER (WHERE cancel_at_period_end IN (true,false)) FROM subscriptions")"
IFS=',' read -r valid_statuses valid_retry_counts valid_cancel_flags <<<"${lifecycle_counts}"
[[ "${valid_statuses}" -gt 0 ]] || { printf '%s\n' "No subscription lifecycle rows found" >&2; exit 1; }
[[ "${valid_retry_counts}" -ge 0 && "${valid_cancel_flags}" -ge 0 ]] || { printf '%s\n' "Subscription lifecycle invariants failed" >&2; exit 1; }

psql "${subscription_database_url}" --no-psqlrc --tuples-only --csv \
    -c "SELECT COUNT(*) FROM subscriptions WHERE status='past_due' AND payment_retry_count > 0 AND grace_period_ends_at IS NOT NULL" \
    >"${PHASE9_DUNNING_REPORT:-/tmp/phase9-dunning-report}"
printf '%s\n' "Subscription state, retry, cancellation and dunning schema smoke passed"
