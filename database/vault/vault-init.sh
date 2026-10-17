#!/bin/sh
# Vault configuration file generator
# This script generates vault.hcl based on VAULT_ENABLE_SSL

VAULT_HCL="/vault/config/vault.hcl"
ENABLE_SSL="${VAULT_ENABLE_SSL:-false}"

echo "Generating vault.hcl with SSL_ENABLED=$ENABLE_SSL"

if [ "$ENABLE_SSL" = "true" ]; then
    cat > "$VAULT_HCL" <<'EOF'
# Vault configured with MANDATORY TLS 1.3
listener "tcp" {
  address = "0.0.0.0:8500"
  tls_disable = false
  tls_cert_file = "/vault/config/tls/server.crt"
  tls_key_file = "/vault/config/tls/server.key"
  tls_client_ca_file = "/vault/config/tls/root.crt"
  tls_min_version = "tls13"
  tls_max_version = "tls13"
}
EOF
else
    cat > "$VAULT_HCL" <<'EOF'
# Vault configured WITHOUT TLS - using plain HTTP (development mode)
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
echo "vault.hcl generated at $VAULT_HCL"
