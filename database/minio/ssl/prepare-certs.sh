#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

# Este script prepara los certificados para MinIO
# MinIO busca automáticamente los archivos de certificado:
# - public.crt: certificado del servidor
# - private.key: clave privada del servidor
# Estos archivos deben estar en el directorio /etc/minio/certs/ con esos nombres específicos

if [ ! -f "$DIR/generales/server.key" ] || [ ! -f "$DIR/generales/server.crt" ]; then
  echo "[minio-prepare] Error: Certificados no encontrados en $DIR/generales/" >&2
  echo "[minio-prepare] Ejecuta primero: ./01-generate-ca.sh y ./02-generate-server-cert.sh" >&2
  exit 1
fi

# Copiar los certificados con los nombres que MinIO espera
cp "$DIR/generales/server.crt" "$DIR/generales/public.crt"
cp "$DIR/generales/server.key" "$DIR/generales/private.key"

# Establecer permisos (644 para legibilidad por contenedores Docker)
chmod 644 "$DIR/generales/private.key"
chmod 644 "$DIR/generales/public.crt"

echo "[minio-prepare] Certificados preparados para MinIO:"
echo "  - $DIR/generales/public.crt"
echo "  - $DIR/generales/private.key"
