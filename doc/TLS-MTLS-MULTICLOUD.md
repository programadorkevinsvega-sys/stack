# TLS/mTLS multicloud

La implementación de transporte seguro vive en `crates/common/src/tls.rs`. Las
aplicaciones no conocen los SDK de AWS, Azure o Vault: construyen una
`ServiceTlsConfig` y reutilizan la misma identidad para servidor gRPC, clientes
gRPC.

## Separación de responsabilidades

- `TLS_MODE=disabled|tls|mtls` define la política de transporte.
- `TLS_CERT_PROVIDER=local|aws|azure|vault` define el origen del certificado.
- Las variables de entorno contienen referencias, nunca certificados o claves.
- Un valor desconocido, material inválido, certificado próximo a expirar o URL
  insegura provoca un error de arranque. No existe fallback a texto plano.

En `mtls`, tonic configura la CA de clientes en cada servidor y presenta la
identidad del servicio en cada cliente. Los endpoints gRPC deben usar `https://`.

Los endpoints HTTP públicos continúan terminando TLS en el balanceador/ingress.
Esta capa protege exclusivamente el tráfico interno gRPC entre microservicios.
PostgreSQL, Redis, NATS/JetStream y Vault no reciben configuración TLS/mTLS de
`ServiceTlsConfig`: mantienen el esquema definido en `DATABASE_URL`,
`REDIS_URL`, `NATS_URL` y `VAULT_ADDR`, respectivamente.

## Bundle almacenado

AWS Secrets Manager, Azure Key Vault Secrets y Vault KV v2 usan un objeto JSON
atómico por servicio:

```json
{
  "certificate_chain_pem": "-----BEGIN CERTIFICATE-----\n...",
  "private_key_pem": "-----BEGIN PRIVATE KEY-----\n...",
  "ca_bundle_pem": "-----BEGIN CERTIFICATE-----\n..."
}
```

`expires_at` y `version` son opcionales. La expiración real también se obtiene
del certificado X.509. Certificado, clave y CA se validan como un conjunto antes
de abrir sockets.

## Proveedores

### Local

```env
TLS_MODE=mtls
TLS_CERT_PROVIDER=local
TLS_DIR=certs
```

Cada servicio lee:

```text
certs/<service>/server.crt
certs/<service>/server.key
certs/<service>/ca.crt
```

Generación local:

```bash
./scripts/generate-dev-certs.sh
```

### AWS

```env
TLS_MODE=mtls
TLS_CERT_PROVIDER=aws
TLS_CERT_NAMESPACE=prod/nexus/tls
AWS_REGION=us-east-1
```

La referencia resultante es
`prod/nexus/tls/<service>`. Se puede reemplazar globalmente con
`AWS_TLS_SECRET_ID` o por servicio con
`<SERVICE>_TLS_SECRET_ID`. La autenticación usa la cadena estándar del SDK de
AWS. El rol requiere únicamente `secretsmanager:GetSecretValue` y, para una CMK,
`kms:Decrypt`.

### Azure

```env
TLS_MODE=mtls
TLS_CERT_PROVIDER=azure
TLS_CERT_NAMESPACE=prod-nexus-tls
AZURE_KEY_VAULT_URL=https://nexus-prod.vault.azure.net
AZURE_AUTH_MODE=workload_identity
```

Se soportan `managed_identity`, `workload_identity` y `developer_tools`. La
identidad necesita permiso de lectura del secreto. El secreto debe contener una
clave privada exportable; una clave HSM/no exportable no se puede entregar a
rustls.

### HashiCorp Vault

El `docker-compose.yml` incluye Vault 1.21.4 con almacenamiento persistente y un
servicio de bootstrap. En el primer arranque, el bootstrap:

1. Inicializa y hace unseal de Vault.
2. Crea la CA local y monta `pki-internal`.
3. Configura el rol `nexus-service-mtls` para certificados de servidor y cliente.
4. Crea una política mínima y el AppRole `nexus-services`.
5. Escribe RoleID y SecretID en `.runtime/vault/`.
6. Importa contraseñas, tokens y claves a Vault KV y genera una proyección
   `runtime.env` protegida para los binarios Rust.
7. Permanece activo para hacer unseal automáticamente después de reinicios.

No requiere ejecutar comandos de Vault:

```bash
docker compose up -d --wait vault vault-bootstrap
```

El perfil local para los microservicios es:

```env
TLS_MODE=mtls
TLS_CERT_PROVIDER=vault
TLS_DNS_SUFFIX=svc.internal
TLS_ALT_NAMES=localhost
TLS_IP_SANS=127.0.0.1
VAULT_ADDR=http://127.0.0.1:8200
VAULT_AUTH_METHOD=approle
VAULT_APPROLE_ROLE_ID_FILE=.runtime/vault/role-id
VAULT_APPROLE_SECRET_ID_FILE=.runtime/vault/secret-id
VAULT_TLS_SOURCE=pki
VAULT_PKI_MOUNT=pki-internal
VAULT_PKI_ROLE=nexus-service-mtls
VAULT_PKI_TTL=24h
```

También se soporta `VAULT_TLS_SOURCE=kv`. Los métodos de autenticación son
`token_file`, `kubernetes` y `approle`; los tokens, JWT y SecretID se leen de
archivos, no de `.env`. `VAULT_ADDR` se consume tal como esté configurado;
activar `TLS_MODE` no cambia el transporte hacia Vault.

La automatización local guarda la clave de unseal y el token raíz bajo
`.runtime/vault/`, ruta excluida de Git. Es una conveniencia de desarrollo de un
solo nodo y no debe desplegarse en producción. En producción se debe usar
auto-unseal con un KMS/HSM, almacenamiento HA y distribución externa de
credenciales AppRole.

`./run-all.sh --build` inicia Vault, espera el bootstrap y después levanta los
binarios Rust; no hay un paso manual de inicialización de Vault.

La gestión de secretos operativos se documenta en `VAULT-SECRETS.md`.

## Rotación y observabilidad

El material se mantiene en memoria, se comparte entre clones de configuración y
se vuelve a consultar después de `TLS_CERT_REFRESH_SECONDS`. Nunca se registra
el contenido del bundle. Se publican:

- `tls_material_load_total{service,provider}`
- `tls_material_load_errors_total{service,provider}`
- `tls_material_validation_errors_total{service,provider}`
- `tls_certificate_expiry_seconds{service,provider}`

Los canales gRPC creados después de una renovación utilizan el bundle nuevo.
Los listeners del servidor usan un `TlsAcceptor` por conexión y consultan el
material cacheado/renovable antes de cada handshake; así la identidad nueva se
aplica sin rebinding ni reinicio. Las conexiones existentes conservan el
handshake ya negociado y se drenan mediante el shutdown controlado. Los
clientes NATS tienen callback de autenticación por reconexión y un supervisor
que solicita una reconexión cuando cambia la configuración.

## Identidad y autorización

Cada certificado debe ser único por servicio, contener SANs acordes a los hosts
de los endpoints y permitir `serverAuth` y `clientAuth`. mTLS autentica la carga
de trabajo; el JWT de servicio existente continúa aplicando autorización y
rotación lógica de credenciales.
