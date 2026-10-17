# Certificados TLS para Vault

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/root.srl` - Serial number file (generado automáticamente)

Rutas dentro del contenedor:

- `/vault/config/tls/server.crt`
- `/vault/config/tls/server.key`
- `/vault/config/tls/root.crt`

## Scripts disponibles

- `01-generate-ca.sh`
  Crea la Autoridad Certificadora (CA) raíz.
  
- `02-generate-server-cert.sh`
  Genera el certificado del servidor con SANs DNS/IP configurables.
  
- `03-renew-server-cert.sh`
  Regenera el certificado del servidor si cambiaron IPs o dominios.
  
- `04-generate-client-cert.sh`
  Genera certificados de cliente para autenticación mTLS.

## Uso rápido

### 1. Generar la CA raíz

```bash
cd database/vault/ssl
./01-generate-ca.sh
```

### 2. Generar certificado del servidor

```bash
./02-generate-server-cert.sh
# El script pregunta por:
# - IP de Tailscale
# - IP privada VPC (opcional)
# - IPs/dominios adicionales (opcional)
```

O pasar IPs directamente:

```bash
./02-generate-server-cert.sh 100.121.34.76 10.0.0.4
```

### 3. Generar certificados de cliente (opcional)

```bash
./04-generate-client-cert.sh
```

### 4. Renovar certificado de servidor

```bash
./03-renew-server-cert.sh 100.121.34.76 10.0.0.4
```

## Variables de entorno

```bash
# CA
CA_DAYS=3650
CA_CN=Vertercloud-Vault-Root-CA

# Server
SERVER_DAYS=825
SERVER_CN=vault-server

# Vault
VAULT_ADDR=https://0.0.0.0:8500
VAULT_API_ADDR=https://0.0.0.0:8500
VAULT_UI=true
```

## Configuración en vault.hcl

El archivo `vault.hcl` ya está configurado para usar TLS:

```hcl
listener "tcp" {
  address = "0.0.0.0:8500"
  tls_disable = false
  tls_cert_file = "/vault/config/tls/server.crt"
  tls_key_file = "/vault/config/tls/server.key"
  tls_client_ca_file = "/vault/config/tls/root.crt"
}
```

## Puertos disponibles

- **8500**: HTTPS (HTTPS) - Puerto único con TLS obligatorio

## Notas importantes

- **HTTPS es obligatorio**: Vault está configurado para requerir TLS.
- **La CA es la base de confianza**: Una vez creada, NO debería regenerarse.
- **El certificado del servidor se puede renovar**: Sin invalidar la CA.
- **vault.hcl controla TLS**: Editar si necesitas cambiar configuración.
- **Permisos**: Los archivos se generan con permisos apropiados (600 para claves).

## Primera inicialización de Vault

Después de generar los certificados:

```bash
# 1. Levanta el contenedor
docker-compose up -d vault

# 2. Obtén el token raíz (guárdalo en lugar seguro)
docker exec vault vault operator init

# 3. Desbloquea Vault
docker exec vault vault operator unseal

# 4. Accede a Vault CLI
docker exec -it vault vault login <TOKEN>
```

## Conexión desde clientes

### CLI de Vault:
```bash
# Configurar URL de Vault
export VAULT_ADDR=https://vault:8500
export VAULT_CACERT=/ruta/a/root.crt

# Conectar
vault login -method=token token=<TOKEN>
```

### Python:
```python
import hvac
import ssl

# Crear contexto SSL
ssl_context = ssl.create_default_context()
ssl_context.load_verify_locations('/ruta/a/root.crt')

client = hvac.Client(
    url='https://vault:8500',
    verify=ssl_context
)

# Autenticar
client.auth.token.login(token='<TOKEN>')
```

### API REST:
```bash
curl \
  --cacert /ruta/a/root.crt \
  --header "X-Vault-Token: <TOKEN>" \
  https://vault:8500/v1/secret/path
```

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**Vault rechaza conexión**
- Verifica que los certificados estén en `generales/`
- Verifica vault.hcl tiene rutas correctas
- Reinicia: `docker-compose restart vault`

**Cliente no confía en certificado**
- Usa: `--cacert /ruta/a/root.crt` en curl
- O coloca `root.crt` en el CA store del sistema

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
- Reinicia Vault

**Vault sigue sin unirse al iniciar**
- Comprueba que los logs de Vault usando: `docker logs vault`
- Verifica que el puerto 8500 esté disponible
