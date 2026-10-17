#!/bin/bash
set -euo pipefail

# 03 - Desellar Vault (unseal)
# Usa las keys del archivo generado por 02-init.sh
# Opcional: pasar keys manuales como argumentos
#
# Uso:
#   ./03-unseal.sh                          # usa keys del archivo de init
#   ./03-unseal.sh "<key1>" "<key2>" "<key3>"  # keys manuales

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
KEYS_FILE="$SCRIPT_DIR/volumes/vault-keys.txt"
VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8500}"

# Obtener keys: argumentos > archivo de init
if [ $# -ge 3 ]; then
  UNSEAL_KEYS=("$@")
else
  if [ ! -f "$KEYS_FILE" ]; then
    echo "❌ No se encontró $KEYS_FILE"
    echo "   Ejecuta primero 02-init.sh o pasa las keys como argumentos"
    exit 1
  fi
  UNSEAL_KEYS=($(python3 -c "
import json
data = json.load(open('$KEYS_FILE'))
for k in data.get('unseal_keys_b64', []):
    print(k)
" 2>/dev/null))
  if [ ${#UNSEAL_KEYS[@]} -lt 3 ]; then
    echo "❌ El archivo $KEYS_FILE no contiene suficientes keys"
    exit 1
  fi
fi

echo "🔓 Desellando Vault (necesitas 3 de 5 keys)..."
for i in 0 1 2; do
  echo "  Key $((i+1))/${#UNSEAL_KEYS[@]}..."
  docker exec vault vault operator unseal "${UNSEAL_KEYS[$i]}" > /dev/null 2>&1
done

echo "✅ Vault desellado"
echo "------------------------------------------"
echo "🔍 Estado:"
docker exec vault vault status

echo ""
echo "👉 Siguiente: 04-login.sh"
