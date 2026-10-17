#!/bin/bash

# Script de restauración para Storage de Supabase
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

# Verificar argumentos
if [ $# -eq 0 ]; then
    error "Uso: $0 <archivo_backup>"
    echo "Ejemplo: $0 storage-backup-20231201-120000.tar.gz"
    echo ""
    echo "Para listar backups disponibles:"
    echo "make storage-backup-list"
    exit 1
fi

BACKUP_FILE="$1"

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

# Verificar si el archivo de backup existe
if ! docker exec supabase-storage-replica test -f "/var/lib/storage/backups/$BACKUP_FILE"; then
    error "El archivo de backup '$BACKUP_FILE' no existe en el contenedor"
fi

log "Iniciando restauración de storage..."
log "Archivo de backup: $BACKUP_FILE"

# Confirmar restauración
warn "⚠️  ADVERTENCIA: Esta operación sobrescribirá los datos actuales del storage"
warn "⚠️  Se recomienda hacer un backup antes de continuar"
echo ""
read -p "¿Estás seguro de que quieres continuar? (y/N): " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    log "Operación cancelada por el usuario"
    exit 0
fi

# Parar el contenedor de storage replica temporalmente
log "🛑 Parando contenedor de storage replica..."
docker stop supabase-storage-replica

# Crear backup de seguridad antes de restaurar
log "💾 Creando backup de seguridad..."
SAFETY_BACKUP="safety-backup-$(date +%Y%m%d-%H%M%S).tar.gz"
docker start supabase-storage-replica
docker exec supabase-storage-replica /bin/bash -c "cd /var/lib/storage && tar -czf /var/lib/storage/backups/$SAFETY_BACKUP . --exclude=backups"
log "✅ Backup de seguridad creado: $SAFETY_BACKUP"

# Parar el contenedor nuevamente
docker stop supabase-storage-replica

# Limpiar directorio de storage actual
log "🧹 Limpiando directorio de storage actual..."
docker run --rm -v supabase-docker_storage_replica_data:/var/lib/storage alpine sh -c "rm -rf /var/lib/storage/*"

# Restaurar desde backup
log "🔄 Restaurando desde backup..."
docker run --rm -v supabase-docker_storage_replica_data:/var/lib/storage -v supabase-docker_storage_backup_data:/backups alpine sh -c "cd /var/lib/storage && tar -xzf /backups/$BACKUP_FILE"

# Iniciar el contenedor
log "🚀 Iniciando contenedor de storage replica..."
docker start supabase-storage-replica

# Verificar que el contenedor esté corriendo
sleep 5
if docker ps --format "table {{.Names}}" | grep -q "supabase-storage-replica"; then
    log "✅ Contenedor iniciado correctamente"
else
    error "❌ Error al iniciar el contenedor"
fi

# Verificar integridad de los datos
log "🔍 Verificando integridad de los datos..."
FILE_COUNT=$(docker exec supabase-storage-replica find /var/lib/storage -type f | wc -l)
log "📊 Archivos restaurados: $FILE_COUNT"

# Mostrar información de la restauración
log "📊 Información de la restauración:"
echo "  - Archivo restaurado: $BACKUP_FILE"
echo "  - Backup de seguridad: $SAFETY_BACKUP"
echo "  - Archivos restaurados: $FILE_COUNT"
echo "  - Fecha: $(date)"
echo "  - Contenedor: supabase-storage-replica"

log "🎉 Restauración de storage completada exitosamente"
log "💡 Para verificar el estado: make storage-replication-status"
