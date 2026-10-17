#!/bin/bash

# Script de backup automático para Storage de Supabase
# Autor: Supabase Docker Project
# Fecha: $(date)

set -e

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Función para logging
log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING: $1${NC}"
}

error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR: $1${NC}"
    exit 1
}

# Verificar si Docker está corriendo
if ! docker info >/dev/null 2>&1; then
    error "Docker no está corriendo"
fi

# Verificar si el contenedor de storage replica existe
if ! docker ps -a --format "table {{.Names}}" | grep -q "supabase-storage-replica"; then
    error "El contenedor supabase-storage-replica no existe. Ejecuta 'make start-storage-replication' primero"
fi

# Verificar si el contenedor está corriendo
if ! docker ps --format "table {{.Names}}" | grep -q "supabase-storage-replica"; then
    error "El contenedor supabase-storage-replica no está corriendo"
fi

# Crear directorio de backups si no existe
log "Creando directorio de backups..."
docker exec supabase-storage-replica mkdir -p /var/lib/storage/backups

# Generar nombre de backup con timestamp
BACKUP_NAME="storage-backup-$(date +%Y%m%d-%H%M%S).tar.gz"
BACKUP_PATH="/var/lib/storage/backups/$BACKUP_NAME"

log "Iniciando backup de storage..."
log "Nombre del backup: $BACKUP_NAME"

# Ejecutar backup
if docker exec supabase-storage-replica /bin/bash -c "cd /var/lib/storage && tar -czf $BACKUP_PATH . --exclude=backups"; then
    log "✅ Backup completado exitosamente"
    
    # Obtener tamaño del backup
    BACKUP_SIZE=$(docker exec supabase-storage-replica du -h "$BACKUP_PATH" | cut -f1)
    log "📦 Tamaño del backup: $BACKUP_SIZE"
    
    # Listar backups disponibles
    log "📋 Backups disponibles:"
    docker exec supabase-storage-replica ls -lah /var/lib/storage/backups/
    
    # Limpiar backups antiguos (mantener solo los últimos 7)
    log "🧹 Limpiando backups antiguos (manteniendo los últimos 7)..."
    docker exec supabase-storage-replica /bin/bash -c "cd /var/lib/storage/backups && ls -t | tail -n +8 | xargs -r rm -f"
    
    log "✅ Proceso de backup completado"
else
    error "❌ Error durante el backup"
fi

# Mostrar información del backup
log "📊 Información del backup:"
echo "  - Nombre: $BACKUP_NAME"
echo "  - Ruta: $BACKUP_PATH"
echo "  - Tamaño: $BACKUP_SIZE"
echo "  - Fecha: $(date)"
echo "  - Contenedor: supabase-storage-replica"

log "🎉 Backup de storage completado exitosamente"
