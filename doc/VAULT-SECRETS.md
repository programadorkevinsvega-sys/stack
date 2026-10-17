# Secretos críticos en Vault

> Reconciliado con el bootstrap y los servicios actuales el 2026-07-25.

Vault KV v2 es la fuente de verdad para contraseñas, peppers, tokens y claves de
proveedores. El `.env` conserva solamente configuración no sensible, selección
de proveedores y rutas a archivos protegidos.

## Valores administrados

La lista canónica está en `vault/runtime-secret-keys.txt` e incluye:

- Contraseña de PostgreSQL, `DATABASE_URL` administrativa y una URL para la
  base independiente de cada microservicio.
- Contraseñas y URLs autenticadas de Redis y NATS.
- Contraseña de acceso a pgAdmin y API key de Qdrant.
- `API_KEY_PEPPER`, secretos JWT y `AUDIT_HMAC_SECRET`.
- Credenciales de Resend, PayPal, Cloudflare R2, Qdrant y OpenRouter.
- `EMBEDDING_API_KEY` para `qwen/qwen3-embedding-8b` mediante la API
  OpenAI-compatible de OpenRouter.
- `OBSERVABILITY_USER_HASH_SALT`, usado para pseudonimizar `user_id` en
  métricas y nunca para reconstruir la identidad.
- Credenciales AWS estáticas si excepcionalmente estuvieran configuradas.

Regiones, endpoints, puertos, nombres de buckets, identificadores KMS, issuers y
subjects no son secretos y permanecen en `.env`.

## Flujo automático

1. `run-all.sh` ejecuta `scripts/migrate-env-secrets-to-vault.sh`.
2. El script retira valores críticos heredados de `.env` y los coloca en
   `.runtime/vault/seed.env` con permisos `0600`.
3. `vault-bootstrap` inicializa y desbloquea Vault, habilita KV v2 en `secret/`
   e importa cada valor bajo `secret/dev/nexus/runtime/<VARIABLE>`.
4. Cada escritura se verifica antes de vaciar el archivo semilla.
5. Vault genera `.runtime/vault/runtime.env` con permisos `0600`.
6. `common::AppConfig::from_env()` carga esa proyección antes de construir la
   configuración de cada microservicio.

Con `LOCAL_DATABASES_ENABLED=true`, el bootstrap genera automáticamente las
credenciales internas que falten y las conserva en Vault. Las bases
`gateway_db`, `router_db`, `credentials_db`, `auth_db`, `keys_db`,
`catalog_db`, `subscription_db`, `payments_db`, `billing_db`, `usage_db`,
`invoicing_db`, `customer_bff_db` y `files_db` se crean en el mismo contenedor
PostgreSQL. `admin-bff` y `s3-gateway` no requieren una base de negocio propia;
delegan autorización a sus servicios internos.
Cada base usa únicamente su esquema `public`.

El AppRole de los servicios solo puede leer
`secret/data/dev/nexus/runtime/*` y emitir certificados con el rol PKI
autorizado. No recibe permisos de escritura, administración ni acceso al token
raíz.

## Arranque

```bash
./run-all.sh --build
```

El flag `--build` conserva compatibilidad con el flujo local, pero la
compilación de producción corresponde a `./build-all.sh`; `run-all.sh` ejecuta
binarios ya compilados y no inicia microservicios en Docker. No se deben volver
a escribir secretos directamente en `.env`. Para rotaciones,
se actualiza el valor correspondiente en Vault KV y se reinicia
`vault-bootstrap` para regenerar la proyección protegida.

La proyección `runtime.env`, el SecretID, el token raíz y la clave de unseal se
encuentran bajo `.runtime/vault/`, excluido de Git. Esta distribución automática
es exclusivamente para desarrollo local. Producción debe usar autenticación de
carga de trabajo y auto-unseal administrado, con rotación y auditoría de Vault;
nunca se copia `runtime.env` a imágenes ni al frontend.
