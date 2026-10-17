#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

# Crear directorio generales si no existe
mkdir -p "$DIR/generales"

CA_KEY="${CA_KEY:-$DIR/generales/root.key}"
CA_CERT="${CA_CERT:-$DIR/generales/root.crt}"
CA_DAYS="${CA_DAYS:-3650}"
CA_CN="${CA_CN:-Vertercloud-NATS-Root-CA}"

if [ -f "$CA_KEY" ] || [ -f "$CA_CERT" ]; then
  echo "[nats-ca] root.key/root.crt already exist in $DIR" >&2
  echo "[nats-ca] remove them first if you really want to recreate the CA" >&2
  exit 1
fi

openssl req \
  -x509 \
  -newkey rsa:4096 \
  -sha256 \
  -nodes \
  -days "$CA_DAYS" \
  -keyout "$CA_KEY" \
  -out "$CA_CERT" \
  -subj "/CN=$CA_CN"

chmod 644 "$CA_KEY"
chmod 644 "$CA_CERT"

echo "[nats-ca] created:"
echo "  - $CA_KEY"
echo "  - $CA_CERT"
echo ""
echo "[*] Los certificados generales están en: $DIR/generales/"
