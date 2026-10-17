#!/bin/bash
set -euo pipefail

# 04 - Login en Vault con Root Token
# Usa el token del archivo de init o pásalo como argumento
#
# Uso:
#   ./04-login.sh              # usa token del archivo
#   ./04-login.sh "s.xxx..."    # token manual

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
KEYS_FILE="$SCRIPT_DIR/volumes/vault-keys.txt"
VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8500}"

if [ $# -ge 1 ]; then
  ROOT_TOKEN="$1"
else
  if [ ! -f "$KEYS_FILE" ]; then
    echo "❌ No se encontró $KEYS_FILE"
    echo "   Ejecuta primero 02-init.sh o pasa el token como argumento"
    exit 1
  fi
  ROOT_TOKEN=$(python3 -c "
import json
data = json.load(open('$KEYS_FILE'))
print(data.get('root_token', ''))
")
  if [ -z "$ROOT_TOKEN" ]; then
    echo "❌ No se encontró root_token en $KEYS_FILE"
    exit 1
  fi
fi

echo "🔐 Iniciando sesión en Vault..."
docker exec -e VAULT_ADDR="$VAULT_ADDR" vault vault login "$ROOT_TOKEN" > /dev/null 2>&1

echo "✅ Sesión iniciada correctamente"
echo ""
echo "👉 Estado completo: 05-status.sh para verificar"
echo "👉 Para probar: vault secrets list"
