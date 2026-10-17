#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

# Este script prepara los certificados para MongoDB
# MongoDB requiere un archivo PEM que contenga la clave privada y el certificado

if [ ! -f "$DIR/generales/server.key" ] || [ ! -f "$DIR/generales/server.crt" ]; then
  echo "[mongodb-prepare] Error: Certificados no encontrados en $DIR/generales/" >&2
  echo "[mongodb-prepare] Ejecuta primero: ./01-generate-ca.sh y ./02-generate-server-cert.sh" >&2
  exit 1
fi

# Crear archivo PEM combinando la clave y el certificado
cat "$DIR/generales/server.key" "$DIR/generales/server.crt" > "$DIR/generales/server.pem"
chmod 644 "$DIR/generales/server.pem"

echo "[mongodb-prepare] Certificados preparados:"
echo "  - $DIR/generales/server.pem (clave + certificado)"
echo "  - $DIR/generales/root.crt (CA)"
