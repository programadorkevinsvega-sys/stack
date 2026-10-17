#!/bin/bash
#
# 📤 EXPORTAR-TODO - Script para empacar todos los datos para llevar a otro PC
# Crea un tarball con TODOS los datos de base de datos y certificados
# Uso: ./export-all-data.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKUP_NAME="n8n-database-backup-$(date +%Y%m%d-%H%M%S).tar.gz"
EXCLUDE_FILE="/tmp/exclude-patterns.txt"

echo "═══════════════════════════════════════════════════════════════"
echo "  📤 EMPACANDO TODOS LOS DATOS DE BASE DE DATOS"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Crear archivo de exclusiones
cat > "$EXCLUDE_FILE" << 'EOF'
.git
.gitignore
node_modules
*.log
logs/
*.swp
*.swo
*~
.DS_Store
Thumbs.db
EOF

echo "📦 Comprimiendo..."
echo "   Incluye:"
echo "   ✓ Todos los datos en volumes/"
echo "   ✓ Todos los certificados SSL"
echo "   ✓ Todos los docker-compose.*.yml"
echo "   ✓ Scripts de configuración"
echo ""

# Crear el tarball
tar --exclude-from="$EXCLUDE_FILE" \
    --exclude="./volumes/pgadmin" \
    --exclude="./volumes/*/data/.cache" \
    -czf "$BACKUP_NAME" \
    -C "$(dirname "$SCRIPT_DIR")" \
    "database/" \
    2>&1 | grep -v "tar: Removing leading"

SIZE=$(du -h "$BACKUP_NAME" | cut -f1)

echo ""
echo "✅ Backup completado"
echo ""
echo "📊 Información del backup:"
echo "   Archivo: $BACKUP_NAME"
echo "   Tamaño: $SIZE"
echo "   Ubicación: $(pwd)/$BACKUP_NAME"
echo ""
echo "🚀 Para restaurar en otro PC:"
echo "   1. Copia el archivo: $BACKUP_NAME"
echo "   2. En el nuevo PC:"
echo "      tar xzf $BACKUP_NAME"
echo "      cd database/"
echo "      ./fix-all-permissions.sh"
echo "      docker compose -f docker-compose.postgres.yml up -d"
echo "      docker compose -f docker-compose.redis.yml up -d"
echo "      ... etc para cada servicio"
echo ""
