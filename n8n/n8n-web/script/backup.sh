#!/bin/bash
# Script de Respaldo Profesional para n8n (DB Externa + Variables)

# Rutas
BASE_DIR="/opt/n8n-cluster"
BACKUP_DIR="$BASE_DIR/backups"
ENV_FILE="$BASE_DIR/.env"
DATE=$(date +"%Y%m%d_%H%M%S")

# Crear directorio de backups si no existe
mkdir -p $BACKUP_DIR

echo "Iniciando respaldo de n8n..."

# Cargar variables del .env para usarlas en el comando de Postgres
set -o allexport
source $ENV_FILE
set +o allexport

# 1. Respaldo de la base de datos PostgreSQL externa
# Usamos un contenedor efímero de postgres para no tener que instalar dependencias en el host
echo "Respaldando base de datos..."
docker run --rm -e PGPASSWORD=$DB_POSTGRESDB_PASSWORD postgres:16-alpine \
    pg_dump -h $DB_POSTGRESDB_HOST -p $DB_POSTGRESDB_PORT -U $DB_POSTGRESDB_USER -d $DB_POSTGRESDB_DATABASE \
    > $BACKUP_DIR/n8n_db_$DATE.sql

# 2. Copiar archivo .env (CRÍTICO: contiene la N8N_ENCRYPTION_KEY)
echo "Respaldando archivo de configuración (.env)..."
cp $ENV_FILE $BACKUP_DIR/env_backup_$DATE.txt

# 3. Comprimir todo en un solo archivo
echo "Comprimiendo archivos..."
tar -czvf $BACKUP_DIR/n8n_backup_$DATE.tar.gz -C $BACKUP_DIR n8n_db_$DATE.sql env_backup_$DATE.txt

# 4. Limpiar archivos temporales
rm $BACKUP_DIR/n8n_db_$DATE.sql $BACKUP_DIR/env_backup_$DATE.txt

# 5. Rotación de backups: Eliminar respaldos con más de 7 días de antigüedad
find $BACKUP_DIR -type f -name "n8n_backup_*.tar.gz" -mtime +7 -exec rm {} \;

echo "✅ Respaldo completado exitosamente: $BACKUP_DIR/n8n_backup_$DATE.tar.gz"