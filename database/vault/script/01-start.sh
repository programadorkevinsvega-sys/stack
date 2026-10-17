#!/bin/bash
set -euo pipefail

# 01 - Construir imagen e iniciar contenedor de Vault
SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "🚀 Construyendo imagen de Vault..."
docker compose -f "$SCRIPT_DIR/docker-compose.vault.yml" build

echo "✅ Iniciando contenedor..."
docker compose -f "$SCRIPT_DIR/docker-compose.vault.yml" up -d

echo "⏳ Esperando que Vault esté listo..."
sleep 5

echo "------------------------------------------"
docker ps --filter "name=vault" --format "table {{.ID}}\t{{.Status}}\t{{.Names}}"
echo ""
echo "👉 Siguiente: 02-init.sh"
