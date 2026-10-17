#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

# Este script prepara los certificados para Kafka en formato PKCS12 (keystores y truststores)
# Kafka requiere:
# - keystore.p12: contiene la clave privada y el certificado del servidor
# - truststore.p12: contiene el certificado raíz (CA)

if [ ! -f "$DIR/generales/server.key" ] || [ ! -f "$DIR/generales/server.crt" ] || [ ! -f "$DIR/generales/root.crt" ]; then
  echo "[kafka-prepare] Error: Certificados no encontrados en $DIR/generales/" >&2
  echo "[kafka-prepare] Ejecuta primero: ./01-generate-ca.sh y ./02-generate-server-cert.sh" >&2
  exit 1
fi

KEYSTORE_PASSWORD="${KAFKA_KEYSTORE_PASSWORD:-Kevins1612522026}"
TRUSTSTORE_PASSWORD="${KAFKA_TRUSTSTORE_PASSWORD:-Kevins1612522026}"

echo "[kafka-prepare] Preparando certificados para Kafka..."
echo ""

# 1. Crear archivo PEM temporal
cat "$DIR/generales/server.key" "$DIR/generales/server.crt" > "$DIR/generales/temp-combined.pem"

# 2. Convertir PEM a PKCS12 (keystore)
echo "[kafka-prepare] Creando keystore..."
openssl pkcs12 \
  -export \
  -in "$DIR/generales/server.crt" \
  -inkey "$DIR/generales/server.key" \
  -out "$DIR/generales/keystore.p12" \
  -name kafka-broker \
  -passout pass:"$KEYSTORE_PASSWORD" \
  -CAfile "$DIR/generales/root.crt" \
  -caname root \
  -chain

# 3. Convertir CA cert a PKCS12 (truststore)
echo "[kafka-prepare] Creando truststore..."
keytool -import \
  -alias ca \
  -file "$DIR/generales/root.crt" \
  -storetype PKCS12 \
  -keystore "$DIR/generales/truststore.p12" \
  -storepass "$TRUSTSTORE_PASSWORD" \
  -noprompt

# 4. Establecer permisos de lectura para Docker (644, no 600)
chmod 644 "$DIR/generales/keystore.p12" "$DIR/generales/truststore.p12"

# 5. Limpiar archivos temporales
rm -f "$DIR/generales/temp-combined.pem"

echo "[kafka-prepare] Certificados preparados:"
echo "  - $DIR/generales/keystore.p12 (servidor)"
echo "  - $DIR/generales/truststore.p12 (CA)"
echo ""
echo "[kafka-prepare] Variables de entorno para usar en docker-compose:"
echo "  KAFKA_KEYSTORE_PASSWORD: $KEYSTORE_PASSWORD"
echo "  KAFKA_TRUSTSTORE_PASSWORD: $TRUSTSTORE_PASSWORD"
