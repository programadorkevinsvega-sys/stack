#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

CA_KEY="${CA_KEY:-$DIR/generales/root.key}"
CA_CERT="${CA_CERT:-$DIR/generales/root.crt}"
CLIENT_KEY="${CLIENT_KEY:-$DIR/generales/client.key}"
CLIENT_CRT="${CLIENT_CRT:-$DIR/generales/client.crt}"
CLIENT_CSR="${CLIENT_CSR:-$DIR/generales/client.csr}"
CLIENT_EXT="${CLIENT_EXT:-$DIR/generales/client.ext}"
CLIENT_DAYS="${CLIENT_DAYS:-825}"
CLIENT_CN="${CLIENT_CN:-redis-client}"

if [ ! -f "$CA_KEY" ] || [ ! -f "$CA_CERT" ]; then
  echo "[redis-client-cert] missing CA files. Run ./01-generate-ca.sh first." >&2
  exit 1
fi

echo "=== Generación de certificado CLIENTE (mTLS) ==="
echo ""

# Preguntar tipo de certificado
read -p "👉 ¿Qué tipo de certificado necesitas?
   1) General - Todos los microservicios usan el mismo certificado
   2) Específico - Un certificado único por microservicio/cliente
   Opción [1/2]: " CERT_TYPE

if [ "$CERT_TYPE" = "2" ]; then
  read -p "👉 Nombre del microservicio/cliente (ej: auth-service, api-gateway): " CLIENT_NAME
  if [ -n "$CLIENT_NAME" ]; then
    CLIENT_KEY="$DIR/generales/${CLIENT_NAME}-client.key"
    CLIENT_CRT="$DIR/generales/${CLIENT_NAME}-client.crt"
    CLIENT_CN="$CLIENT_NAME"
    echo "[*] Generando certificado específico para: $CLIENT_NAME"
  else
    echo "[!] Nombre vacío. Usando certificado general."
  fi
else
  echo "[*] Generando certificado general para todos los microservicios"
fi

echo ""

cat > "$CLIENT_EXT" <<EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
EOF

openssl req \
  -new \
  -newkey rsa:4096 \
  -nodes \
  -keyout "$CLIENT_KEY" \
  -out "$CLIENT_CSR" \
  -subj "/CN=$CLIENT_CN"

openssl x509 \
  -req \
  -sha256 \
  -days "$CLIENT_DAYS" \
  -in "$CLIENT_CSR" \
  -CA "$CA_CERT" \
  -CAkey "$CA_KEY" \
  -CAcreateserial \
  -out "$CLIENT_CRT" \
  -extfile "$CLIENT_EXT"

chmod 644 "$CLIENT_KEY"
chmod 644 "$CLIENT_CRT"
rm -f "$CLIENT_CSR" "$CLIENT_EXT"

echo "[redis-client-cert] created:"
echo "  - $CLIENT_KEY"
echo "  - $CLIENT_CRT"
echo ""
if [ "$CERT_TYPE" = "2" ] && [ -n "$CLIENT_NAME" ]; then
  echo "[*] Usar en $CLIENT_NAME:"
  echo "   tls_cert: /ruta/al/certificado/${CLIENT_NAME}-client.crt"
  echo "   tls_key:  /ruta/al/certificado/${CLIENT_NAME}-client.key"
  echo "   tls_ca:   /ruta/al/certificado/root.crt"
else
  echo "[*] Este certificado puede ser compartido por todos los microservicios"
fi
