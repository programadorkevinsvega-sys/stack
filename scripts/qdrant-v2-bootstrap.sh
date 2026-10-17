#!/usr/bin/env bash
set -euo pipefail

QDRANT_URL="${QDRANT_URL:?QDRANT_URL is required}"
COLLECTION="${QDRANT_EMBEDDING_COLLECTION:-nexus_embeddings_v2}"
[[ "$COLLECTION" =~ ^[A-Za-z0-9_]+_v2$ ]] || { echo "invalid v2 collection name" >&2; exit 2; }
BASE="${QDRANT_URL%/}"
declare -a HEADERS=()
if [[ -n "${QDRANT_API_KEY:-}" ]]; then
  HEADERS+=( -H "api-key: ${QDRANT_API_KEY}" )
fi

metadata="$(curl -sS "${HEADERS[@]}" "$BASE/collections/$COLLECTION")"
status="$?"
if [[ "$status" -ne 0 ]]; then
  echo "Qdrant collection inspection request failed" >&2
  exit 1
fi
http_status="$(curl -sS -o /dev/null -w '%{http_code}' "${HEADERS[@]}" "$BASE/collections/$COLLECTION")"
if [[ "$http_status" == 404 ]]; then
  curl -fsS -X PUT "${HEADERS[@]}" \
    -H 'content-type: application/json' \
    "$BASE/collections/$COLLECTION" \
    -d '{"vectors":{"size":4096,"distance":"Cosine"}}' >/dev/null
elif [[ "$http_status" != 2* ]]; then
  echo "Qdrant collection inspection failed with HTTP $http_status" >&2
  exit 1
elif ! jq -e '.result.config.params.vectors.size == 4096' >/dev/null <<<"$metadata"; then
  echo "Qdrant v2 collection has an incompatible vector dimension" >&2
  exit 1
fi
for field in user_id embedding_vector_id content_hash embedding_model embedding_version strategy_family; do
  curl -fsS -X PUT "${HEADERS[@]}" \
    -H 'content-type: application/json' \
    "$BASE/collections/$COLLECTION/index?wait=true" \
    -d "{\"field_name\":\"$field\",\"field_schema\":\"keyword\"}" >/dev/null
done
echo "Qdrant v2 collection ready: $COLLECTION"
