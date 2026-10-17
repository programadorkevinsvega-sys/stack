#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

# Usar directorio generales para certs del servidor
mkdir -p "$DIR/generales"

CA_KEY="${CA_KEY:-$DIR/generales/root.key}"
CA_CERT="${CA_CERT:-$DIR/generales/root.crt}"
SERVER_KEY="${SERVER_KEY:-$DIR/generales/server.key}"
SERVER_CRT="${SERVER_CRT:-$DIR/generales/server.crt}"
SERVER_CSR="${SERVER_CSR:-$DIR/generales/server.csr}"
SERVER_EXT="${SERVER_EXT:-$DIR/generales/server.ext}"
SERVER_DAYS="${SERVER_DAYS:-825}"
SERVER_CN="${SERVER_CN:-mysql-server}"

if [ ! -f "$CA_KEY" ] || [ ! -f "$CA_CERT" ]; then
  echo "[mysql-server-cert] missing CA files. Run ./01-generate-ca.sh first." >&2
  exit 1
fi

if [ "$#" -eq 0 ]; then
  echo "=== Generación de certificado MySQL ==="
  echo ""
  read -p "👉 Ingresa la IP de Tailscale del VPS (ej: 100.x.y.z): " TAILSCALE_IP
  read -p "👉 Ingresa la IP privada VPC del VPS (ej: 10.0.0.x) [opcional]: " VPC_IP
  
  if [ -z "$TAILSCALE_IP" ]; then
    echo "[!] Error: La IP de Tailscale es obligatoria."
    exit 1
  fi
  
  # Construir lista de IPs base
  IP_LIST="localhost 127.0.0.1 $TAILSCALE_IP"
  if [ -n "$VPC_IP" ]; then
    IP_LIST="$IP_LIST $VPC_IP"
  fi
  
  # Preguntar por IPs/dominios adicionales
  echo ""
  read -p "👉 ¿Deseas agregar IPs o dominios adicionales? [s/N]: " ADD_MORE
  while [ "$ADD_MORE" = "s" ] || [ "$ADD_MORE" = "S" ]; do
    read -p "   Ingresa IP o dominio adicional (ej: mi-dominio.com o 192.168.1.10): " EXTRA
    if [ -n "$EXTRA" ]; then
      IP_LIST="$IP_LIST $EXTRA"
    fi
    read -p "👉 ¿Agregar otro? [s/N]: " ADD_MORE
  done
  
  # Agregar nombres de contenedor Docker para comunicación interna
  IP_LIST="$IP_LIST mysql"
  
  set -- $IP_LIST
  echo ""
  echo "[*] Generando certificado para:"
  for ENTRY in "$@"; do
    echo "    - $ENTRY"
  done
  echo ""
fi

rm -f "$SERVER_CSR" "$SERVER_EXT"

SAN_ENTRIES=""
INDEX=1
for ENTRY in "$@"; do
  case "$ENTRY" in
    *[!0-9.]* | *:* )
      SAN_ENTRIES="${SAN_ENTRIES}DNS.${INDEX} = ${ENTRY}
"
      ;;
    * )
      SAN_ENTRIES="${SAN_ENTRIES}IP.${INDEX} = ${ENTRY}
"
      ;;
  esac
  INDEX=$((INDEX + 1))
done

cat > "$SERVER_EXT" <<EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names

[alt_names]
${SAN_ENTRIES}
EOF

openssl req \
  -new \
  -newkey rsa:4096 \
  -nodes \
  -keyout "$SERVER_KEY" \
  -out "$SERVER_CSR" \
  -subj "/CN=$SERVER_CN"

openssl x509 \
  -req \
  -sha256 \
  -days "$SERVER_DAYS" \
  -in "$SERVER_CSR" \
  -CA "$CA_CERT" \
  -CAkey "$CA_KEY" \
  -CAcreateserial \
  -out "$SERVER_CRT" \
  -extfile "$SERVER_EXT"

chmod 644 "$SERVER_KEY"
chmod 644 "$SERVER_CRT"
rm -f "$SERVER_CSR"

echo "[mysql-server-cert] created:"
echo "  - $SERVER_KEY"
echo "  - $SERVER_CRT"
