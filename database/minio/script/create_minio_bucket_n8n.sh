#!/bin/bash

# 1. Cargar variables desde el archivo .env
if [ -f ../.env ]; then
    export $(grep -v '^#' ../.env | xargs)
else
    echo "❌ Error: No se encontró el archivo .env"
    exit 1
fi

# 2. Configuración (usa lo que hay en tu .env)
MINIO_URL="http://minio:9000"
BUCKET_NAME=${N8N_S3_BUCKET_NAME:-n8n-files}
ACCESS_KEY=${N8N_S3_ACCESS_KEY:-KevinsAdmin}
SECRET_KEY=${N8N_S3_SECRET_KEY:-Kevins1612522026}

echo "📡 Conectando a MinIO en $MINIO_URL..."

# 3. Ejecutar comandos de configuración
docker run --rm --network stack-network \
  --entrypoint /bin/sh \
  minio/mc -c "
    # Crear el alias para conectar
    mc alias set n8n-storage $MINIO_URL $ACCESS_KEY $SECRET_KEY;
    
    # Verificar si el bucket existe, si no, crearlo
    if mc ls n8n-storage/$BUCKET_NAME > /dev/null 2>&1; then
      echo '✅ El bucket \"$BUCKET_NAME\" ya existe.';
    else
      mc mb n8n-storage/$BUCKET_NAME;
      echo '🚀 Bucket \"$BUCKET_NAME\" creado con éxito.';
    fi

    # --- AQUÍ SE PONE LA LÍNEA PARA HACERLO PÚBLICO ---
    echo '🔓 Configurando acceso público para el bucket...'
    mc anonymous set public n8n-storage/$BUCKET_NAME;
    echo '✨ ¡Listo! Ahora los archivos en $BUCKET_NAME son accesibles vía URL.';
  "