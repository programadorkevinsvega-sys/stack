#!/bin/bash
# Script to generate vault.hcl configuration based on VAULT_ENABLE_SSL variable

set -e

VAULT_HCL="/vault/config/vault.hcl"
ENABLE_SSL="${VAULT_ENABLE_SSL:-false}"

echo "Generating vault.hcl with SSL_ENABLED=$ENABLE_SSL"

cat > "$VAULT_HCL" <<'EOF'
EOF

if [ "$ENABLE_SSL" = "true" ]; then
    cat >> "$VAULT_HCL" <<'EOF'
# Vault configured with MANDATORY TLS 1.3
listener "tcp" {
  address = "0.0.0.0:8500"
  tls_disable = false
  tls_cert_file = "/vault/config/tls/server.crt"
  tls_key_file = "/vault/config/tls/server.key"
  tls_client_ca_file = "/vault/config/tls/root.crt"
  # ONLY TLS 1.3 - disable SSL and older versions
  tls_min_version = "tls13"
  tls_max_version = "tls13"
}
EOF
else
    cat >> "$VAULT_HCL" <<'EOF'
# Vault configured WITHOUT TLS - using plain HTTP (use only for development!)
listener "tcp" {
  address = "0.0.0.0:8500"
  tls_disable = true
}
EOF
fi

cat >> "$VAULT_HCL" <<'EOF'

storage "file" {
  path = "/vault/data"
}

ui = true
disable_mlock = true
EOF

chmod 644 "$VAULT_HCL"
echo "vault.hcl generated successfully at $VAULT_HCL"
