#!/bin/bash

# Script de inicialización para Storage de Supabase
# Autor: Supabase Docker Project
# Fecha: $(date)

set -e

# Colores para output
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

info() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')] INFO: $1${NC}"
}

log "🚀 Inicializando Storage de Supabase..."

# Crear directorios necesarios
log "📁 Creando directorios de storage..."
mkdir -p /var/lib/storage/buckets
mkdir -p /var/lib/storage/temp
mkdir -p /var/lib/storage/backups
mkdir -p /var/lib/storage/cache
mkdir -p /var/lib/storage/thumbnails

# Configurar permisos
log "🔐 Configurando permisos..."
chown -R 1000:1000 /var/lib/storage
chmod -R 755 /var/lib/storage

# Crear buckets por defecto
log "🪣 Creando buckets por defecto..."

# Bucket público para assets
mkdir -p /var/lib/storage/buckets/public
echo '{"public": true, "file_size_limit": 52428800, "allowed_mime_types": ["image/*", "video/*", "audio/*", "application/pdf"]}' > /var/lib/storage/buckets/public/config.json

# Bucket privado para documentos
mkdir -p /var/lib/storage/buckets/private
echo '{"public": false, "file_size_limit": 104857600, "allowed_mime_types": ["*/*"]}' > /var/lib/storage/buckets/private/config.json

# Bucket para avatares
mkdir -p /var/lib/storage/buckets/avatars
echo '{"public": true, "file_size_limit": 5242880, "allowed_mime_types": ["image/jpeg", "image/png", "image/gif", "image/webp"]}' > /var/lib/storage/buckets/avatars/config.json

# Bucket para thumbnails
mkdir -p /var/lib/storage/buckets/thumbnails
echo '{"public": true, "file_size_limit": 1048576, "allowed_mime_types": ["image/jpeg", "image/png", "image/webp"]}' > /var/lib/storage/buckets/thumbnails/config.json

# Crear archivo de configuración de storage
log "⚙️ Creando configuración de storage..."
cat > /var/lib/storage/storage-config.json << EOF
{
  "storage": {
    "backend": "file",
    "path": "/var/lib/storage",
    "max_file_size": 52428800,
    "allowed_mime_types": [
      "image/*",
      "video/*",
      "audio/*",
      "application/pdf",
      "text/*",
      "application/json"
    ],
    "transformations": {
      "enabled": true,
      "imgproxy_url": "http://imgproxy:5001",
      "max_width": 2048,
      "max_height": 2048,
      "quality": 85
    },
    "replication": {
      "enabled": false,
      "sync_interval": 300,
      "backup_enabled": true,
      "backup_schedule": "0 2 * * *",
      "backup_retention": 7
    }
  }
}
EOF

# Crear script de limpieza
log "🧹 Creando script de limpieza..."
cat > /var/lib/storage/cleanup.sh << 'EOF'
#!/bin/bash

# Script de limpieza para Storage
# Elimina archivos temporales y cache antiguo

# Limpiar archivos temporales (más de 1 hora)
find /var/lib/storage/temp -type f -mmin +60 -delete 2>/dev/null || true

# Limpiar cache antiguo (más de 24 horas)
find /var/lib/storage/cache -type f -mtime +1 -delete 2>/dev/null || true

# Limpiar thumbnails antiguos (más de 7 días)
find /var/lib/storage/thumbnails -type f -mtime +7 -delete 2>/dev/null || true

# Limpiar backups antiguos (más de 30 días)
find /var/lib/storage/backups -type f -mtime +30 -delete 2>/dev/null || true

echo "Limpieza de storage completada: $(date)"
EOF

chmod +x /var/lib/storage/cleanup.sh

# Crear script de backup
log "💾 Creando script de backup..."
cat > /var/lib/storage/backup.sh << 'EOF'
#!/bin/bash

# Script de backup para Storage
BACKUP_DIR="/var/lib/storage/backups"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BACKUP_FILE="storage-backup-$TIMESTAMP.tar.gz"

# Crear backup
cd /var/lib/storage
tar -czf "$BACKUP_DIR/$BACKUP_FILE" \
    --exclude=backups \
    --exclude=temp \
    --exclude=cache \
    buckets/ thumbnails/ storage-config.json

# Comprimir y verificar
if [ -f "$BACKUP_DIR/$BACKUP_FILE" ]; then
    echo "Backup creado: $BACKUP_FILE"
    echo "Tamaño: $(du -h "$BACKUP_DIR/$BACKUP_FILE" | cut -f1)"
else
    echo "Error: No se pudo crear el backup"
    exit 1
fi

# Limpiar backups antiguos (mantener solo los últimos 7)
cd "$BACKUP_DIR"
ls -t storage-backup-*.tar.gz | tail -n +8 | xargs -r rm -f

echo "Backup completado: $(date)"
EOF

chmod +x /var/lib/storage/backup.sh

# Crear script de monitoreo
log "📊 Creando script de monitoreo..."
cat > /var/lib/storage/monitor.sh << 'EOF'
#!/bin/bash

# Script de monitoreo para Storage
echo "=== Storage Monitor ==="
echo "Fecha: $(date)"
echo ""

# Espacio en disco
echo "📊 Espacio en disco:"
df -h /var/lib/storage
echo ""

# Tamaño de directorios
echo "📁 Tamaño de directorios:"
du -sh /var/lib/storage/* 2>/dev/null | sort -hr
echo ""

# Número de archivos
echo "📄 Número de archivos:"
find /var/lib/storage -type f | wc -l
echo ""

# Archivos más grandes
echo "🔍 Archivos más grandes:"
find /var/lib/storage -type f -exec du -h {} + 2>/dev/null | sort -hr | head -10
echo ""

# Backups disponibles
echo "💾 Backups disponibles:"
ls -lah /var/lib/storage/backups/ 2>/dev/null || echo "No hay backups"
echo ""

echo "Monitoreo completado: $(date)"
EOF

chmod +x /var/lib/storage/monitor.sh

# Crear archivo de estado
log "📝 Creando archivo de estado..."
cat > /var/lib/storage/status.json << EOF
{
  "initialized": true,
  "timestamp": "$(date -Iseconds)",
  "version": "1.0.0",
  "buckets": [
    "public",
    "private", 
    "avatars",
    "thumbnails"
  ],
  "config": {
    "backend": "file",
    "max_file_size": 52428800,
    "replication_enabled": false,
    "backup_enabled": true
  }
}
EOF

# Configurar cron para limpieza automática
log "⏰ Configurando limpieza automática..."
echo "0 2 * * * /var/lib/storage/cleanup.sh" | crontab - 2>/dev/null || warn "No se pudo configurar cron"

# Crear archivo de ejemplo
log "📄 Creando archivo de ejemplo..."
cat > /var/lib/storage/buckets/public/README.txt << EOF
# Bucket Público de Supabase Storage

Este es un bucket público donde puedes almacenar archivos que serán accesibles públicamente.

## Características:
- Público: Sí
- Límite de tamaño: 50MB
- Tipos permitidos: Imágenes, videos, audio, PDF

## Uso:
- Sube archivos a este bucket
- Los archivos serán accesibles en: /storage/v1/object/public/[archivo]
- Usa la API de Supabase para gestionar archivos

## Ejemplo de URL:
http://localhost:8000/storage/v1/object/public/README.txt

Inicializado: $(date)
EOF

log "✅ Storage de Supabase inicializado correctamente"
log "📁 Directorios creados: buckets, temp, backups, cache, thumbnails"
log "🪣 Buckets creados: public, private, avatars, thumbnails"
log "⚙️ Configuración: storage-config.json"
log "🧹 Scripts: cleanup.sh, backup.sh, monitor.sh"
log "📊 Estado: status.json"

info "Para monitorear el storage: /var/lib/storage/monitor.sh"
info "Para hacer backup: /var/lib/storage/backup.sh"
info "Para limpiar: /var/lib/storage/cleanup.sh"
