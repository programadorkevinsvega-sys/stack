#!/bin/bash
set -euo pipefail

# 02 - Inicializar Vault (genera Unseal Keys + Root Token)
# Guarda las claves en ./volumes/vault-keys.txt (¡proteger este archivo!)

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
KEYS_FILE="$SCRIPT_DIR/volumes/vault-keys.txt"
VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8500}"

# Verificar que Vault está accesible (da 501 no-init o 503 sellado, no usar -f)
echo "🔍 Verificando conexión con Vault..."
if ! curl -s "$VAULT_ADDR/v1/sys/health" > /dev/null 2>&1; then
  echo "❌ No se puede conectar a Vault en $VAULT_ADDR"
  echo "   Asegúrate de que el contenedor esté corriendo: 01-start.sh"
  exit 1
fi

# Verificar que no esté ya inicializado
STATUS=$(vault status -format=json 2>/dev/null || echo '{"initialized": false}')
if echo "$STATUS" | grep -q '"initialized": true'; then
  echo "⚠️  Vault ya está inicializado."
  echo "   Si quieres reiniciar, borra el volumen: rm -rf $SCRIPT_DIR/volumes/data/*"
  exit 1
fi

echo "🔐 Inicializando Vault..."
INIT_OUTPUT=$(vault operator init -key-shares=5 -key-threshold=3 -format=json 2>&1)

echo "$INIT_OUTPUT" > "$KEYS_FILE"
echo "✅ Claves guardadas en: $KEYS_FILE"
echo "⚠️  IMPORTANTE: Guarda este archivo en un lugar seguro."

echo ""
echo "===== CLAVES GENERADAS ====="
cat "$KEYS_FILE" | python3 -c "
import json, sys
data = json.load(sys.stdin)
for i, key in enumerate(data.get('unseal_keys_b64', []), 1):
    print(f'  Unseal Key {i}: {key}')
print(f'  Root Token: {data.get(\"root_token\", \"\")}')
"
echo "============================"
echo ""
echo "👉 Siguiente: 03-unseal.sh (necesitas 3 de las 5 keys)"
