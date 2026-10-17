#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CERTS_DIR="${PROJECT_ROOT}/certs"

SERVICES=(
    "api-gateway"
    "auth-service"
    "api-service"
    "credential-manager"
    "key-services"
    "model-router"
    "subscription-service"
    "payment-services"
    "billing-service"
    "usage-service"
    "invoice-service"
    "customer-bff"
)

echo "📜 Generating self-signed TLS certificates for local dev..."
mkdir -p "$CERTS_DIR"

# Generate CA cert and key
if [[ ! -f "$CERTS_DIR/ca.crt" ]] || [[ ! -f "$CERTS_DIR/ca.key" ]]; then
    echo "  → Generating CA certificate..."
    openssl req -x509 -newkey rsa:4096 -days 3650 -nodes \
        -keyout "$CERTS_DIR/ca.key" \
        -out "$CERTS_DIR/ca.crt" \
        -subj "/CN=ai-gateway-local-ca" \
        -addext "basicConstraints=critical,CA:TRUE" \
        -addext "keyUsage=critical,keyCertSign,cRLSign"
else
    echo "  → CA certificate already exists, skipping."
fi

# Generate per-service certs
for service in "${SERVICES[@]}"; do
    SERVICE_DIR="$CERTS_DIR/$service"
    mkdir -p "$SERVICE_DIR"

    if [[ -f "$SERVICE_DIR/server.crt" ]] && [[ -f "$SERVICE_DIR/server.key" ]]; then
        echo "  → $service certificates already exist, skipping."
        continue
    fi

    echo "  → Generating $service certificate..."

    # Generate server key
    openssl genrsa -out "$SERVICE_DIR/server.key" 2048 2>/dev/null

    # Generate CSR
    openssl req -new -key "$SERVICE_DIR/server.key" \
        -out "$SERVICE_DIR/server.csr" \
        -subj "/CN=$service.local" \
        -addext "subjectAltName=DNS:localhost,DNS:$service.local,DNS:$service,IP:127.0.0.1" \
        -addext "keyUsage=critical,digitalSignature,keyEncipherment" \
        -addext "extendedKeyUsage=serverAuth,clientAuth" \
        2>/dev/null

    # Sign with CA
    openssl x509 -req -in "$SERVICE_DIR/server.csr" \
        -CA "$CERTS_DIR/ca.crt" \
        -CAkey "$CERTS_DIR/ca.key" \
        -CAcreateserial \
        -out "$SERVICE_DIR/server.crt" \
        -days 365 \
        -copy_extensions copyall 2>/dev/null

    # Copy CA to service dir
    cp "$CERTS_DIR/ca.crt" "$SERVICE_DIR/ca.crt"

    # Clean up CSR
    rm "$SERVICE_DIR/server.csr"

    echo "    ✓ $service/server.crt, server.key, ca.crt created"
done

echo ""
echo "✅ Certificates generated in $CERTS_DIR"
echo "   CA:     $CERTS_DIR/ca.crt"
echo "   Services under: $CERTS_DIR/<service-name>/"
echo ""
echo "Add to .env or export:"
echo "  TLS_MODE=mtls"
echo "  TLS_CERT_PROVIDER=local"
echo "  TLS_DIR=certs"
