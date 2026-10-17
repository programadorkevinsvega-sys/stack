#!/bin/sh
set -e

# Script de inicio para Vault
# Genera configuración dinámica basada en variables de entorno

/vault-init.sh

# Función para auto-unseal en segundo plano
auto_unseal() {
  if [ -n "$VAULT_UNSEAL_KEY" ]; then
    echo "⚙️ Auto-Unseal configurado. Monitoreando estado de Vault..."
    
    # Determinar el protocolo a usar según la configuración de SSL
    PROTO="http"
    if [ "$VAULT_ENABLE_SSL" = "true" ]; then
      PROTO="https"
    fi
    export VAULT_ADDR="${PROTO}://127.0.0.1:8500"
    export VAULT_SKIP_VERIFY="true"
    
    # Bucle infinito para desellar si Vault se sella (ej. reinicio de contenedor)
    while true; do
      # Comprobamos el estado ignorando errores de conexión iniciales
      STATUS=$(vault status -format=json 2>/dev/null || echo '{"sealed": false}')
      
      # Si está sellado, intentamos desellar
      if echo "$STATUS" | grep -q '"sealed": true'; then
        echo "🔓 Vault sellado. Desellando automáticamente..."
        vault operator unseal "$VAULT_UNSEAL_KEY" > /dev/null
        echo "✅ Vault desellado con éxito."
      fi
      
      sleep 10
    done
  fi
}

# Lanzar proceso de auto-unseal en segundo plano
auto_unseal &

# Ejecuta Vault en modo servidor
exec vault server -config=/vault/config/vault.hcl
