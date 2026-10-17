#!/bin/bash
set -euo pipefail

# Script para abrir Vault sin dejar secretos hardcodeados en el repo.
# Uso:
#   VAULT_UNSEAL_KEY='<clave>' ./database/vault/script/unseal_vault.sh
#   ./database/vault/script/unseal_vault.sh '<clave>' '<root_token_opcional>'

VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8500}"
VAULT_CACERT="${VAULT_CACERT:-/vault/config/tls/root.crt}"

UNSEAL_KEY="${VAULT_UNSEAL_KEY:-${1:-}}"
ROOT_TOKEN="${VAULT_ROOT_TOKEN:-${2:-}}"

if [ -z "$UNSEAL_KEY" ]; then
  echo "Falta la Unseal Key."
  echo ""
  echo "Ejemplos:"
  echo "  VAULT_UNSEAL_KEY='<clave>' ./database/vault/script/unseal_vault.sh"
  echo "  ./database/vault/script/unseal_vault.sh '<clave>' '<root_token_opcional>'"
  exit 1
fi

echo "🔓 Abriendo Vault..."
docker exec \
  -e VAULT_ADDR="$VAULT_ADDR" \
  -e VAULT_CACERT="$VAULT_CACERT" \
  vault vault operator unseal "$UNSEAL_KEY"

echo "------------------------------------------"
echo "🔍 Verificando estado..."
docker exec \
  -e VAULT_ADDR="$VAULT_ADDR" \
  -e VAULT_CACERT="$VAULT_CACERT" \
  vault vault status

if [ -n "$ROOT_TOKEN" ]; then
  echo "------------------------------------------"
  echo "🔐 Guardando login administrativo en la CLI del contenedor..."
  docker exec \
    -e VAULT_ADDR="$VAULT_ADDR" \
    -e VAULT_CACERT="$VAULT_CACERT" \
    vault vault login "$ROOT_TOKEN" > /dev/null
  echo "✅ Login almacenado"
fi

echo "------------------------------------------"
echo "🚀 Estado del contenedor:"
docker ps --filter "name=vault" --format "table {{.ID}}\t{{.Status}}\t{{.Names}}"
