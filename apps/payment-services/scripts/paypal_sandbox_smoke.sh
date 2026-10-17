#!/usr/bin/env bash
set -euo pipefail

: "${PAYPAL_BASE_URL:?PAYPAL_BASE_URL is required}"
: "${PAYPAL_CLIENT_ID:?PAYPAL_CLIENT_ID is required}"
: "${PAYPAL_CLIENT_SECRET:?PAYPAL_CLIENT_SECRET is required}"
: "${PAYPAL_RETURN_URL:?PAYPAL_RETURN_URL is required}"
: "${PAYPAL_CANCEL_URL:?PAYPAL_CANCEL_URL is required}"
: "${PAYPAL_SMOKE_MODE:=order}"
if [[ "${PAYPAL_SMOKE_MODE}" != "order" && "${PAYPAL_SMOKE_MODE}" != "capture" ]]; then
    printf '%s\n' "PAYPAL_SMOKE_MODE must be order or capture" >&2
    exit 2
fi
if [[ "${PAYPAL_SMOKE_MODE}" == "capture" ]]; then
    : "${PAYPAL_ORDER_ID:?PAYPAL_ORDER_ID is required in capture mode after buyer approval}"
fi

token="$(
    curl -fsS \
        -u "${PAYPAL_CLIENT_ID}:${PAYPAL_CLIENT_SECRET}" \
        -d 'grant_type=client_credentials' \
        "${PAYPAL_BASE_URL%/}/v1/oauth2/token" \
        | jq -er '.access_token'
)"

if [[ "${PAYPAL_SMOKE_MODE}" == "order" ]]; then
    order="$(
        curl -fsS \
            -X POST "${PAYPAL_BASE_URL%/}/v2/checkout/orders" \
            -H "Authorization: Bearer ${token}" \
            -H 'Content-Type: application/json' \
            -H "PayPal-Request-Id: phase9-order-$(date +%s%N)" \
            -d "{
              \"intent\":\"CAPTURE\",
              \"purchase_units\":[{\"amount\":{\"currency_code\":\"USD\",\"value\":\"1.00\"}}],
              \"application_context\":{\"return_url\":\"${PAYPAL_RETURN_URL}\",\"cancel_url\":\"${PAYPAL_CANCEL_URL}\",\"user_action\":\"PAY_NOW\"}
            }"
    )"
    jq -e '.id and .status' >/dev/null <<<"${order}"
    echo "${order}" | jq '{id,status,approval_url:(.links[] | select(.rel=="approve") | .href)}'
else
    capture_response="$(curl -fsS -X POST \
        "${PAYPAL_BASE_URL%/}/v2/checkout/orders/${PAYPAL_ORDER_ID}/capture" \
        -H "Authorization: Bearer ${token}" \
        -H 'Content-Type: application/json' \
        -H "PayPal-Request-Id: phase9-capture-${PAYPAL_ORDER_ID}" \
        -d '{}')"
    echo "${capture_response}" | jq '{id,status,purchase_units:(.purchase_units // []) | length}'
fi
