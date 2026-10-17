#!/bin/bash
set -euo pipefail

# 05 - Ver estado completo de Vault
VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8500}"

echo "=== Estado de Vault ==="
vault status 2>/dev/null || echo "❌ No se pudo conectar"

echo ""
echo "=== Sellos activos ==="
vault secrets list 2>/dev/null || echo "(sin login o no disponible)"

echo ""
echo "=== Contenedor ==="
docker ps --filter "name=vault" --format "table {{.ID}}\t{{.Status}}\t{{.Names}}"
