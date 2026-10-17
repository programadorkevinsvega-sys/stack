# AI Gateway — Plan de Entidades y Casos de Uso por Servicio

> Documento reconciliado con el workspace actual el 2026-07-25. Las secciones
> históricas se conservan como referencia; la sección "Estado actual" es la
> fuente de verdad de los servicios existentes.
>
> Generado a partir de los documentos de diseño y arquitectura:
> - `01-arquitectura-microservicios.md`
> - `03-seguridad-checklist.md`
> - `05-reglas-ingenieria.md`
>
> **Ajuste de dominio:** Sin organizaciones ni multi-tenant. Cada usuario es dueño directo de sus API keys, wallet, suscripción y facturación. No hay equipos ni jerarquía — una key se pertenece a sí misma a través del usuario que la creó.
>
> Orden de construcción: 5 crates compartidos → servicios de backend y BFF.
> La separación DDD se aplica por bounded context, pero la distribución física
> de carpetas puede variar por servicio.

---

## Matriz global de transportes

| Origen | Destino | Transporte | Contrato / propósito |
|--------|---------|------------|----------------------|
| Dashboard cliente | `customer-bff` | HTTP/JSON | Frontera pública del dashboard de cliente. |
| Dashboard admin | `admin-bff` | HTTP/JSON | Frontera pública del dashboard administrativo. |
| SDK/S3 cliente | `api-gateway`, `s3-gateway` | HTTP/JSON/S3 | APIs públicas de inferencia y almacenamiento. |
| Servicios internos/BFF | Otro servicio interno | gRPC + Protobuf | Comunicación síncrona interna mediante contratos definidos en `crates/proto`; las APIs públicas son `api-gateway` y `s3-gateway`. |
| Cualquier microservicio | Consumidores asíncronos | NATS | Publicación y consumo de eventos de dominio; no se usa como sustituto de llamadas RPC síncronas. |

Los servicios internos usan gRPC + Protobuf para llamadas síncronas y NATS
JetStream para eventos. `customer-bff` y `admin-bff` consumen sus fuentes de
datos por gRPC. `api-gateway` y `s3-gateway` también son fronteras públicas
HTTP para SDKs y clientes S3.

---

## Orden de construcción

| # | Artefacto | Tipo | Depende de |
|---|-----------|------|------------|
| 1 | `crates/common` | Crate compartido | — |
| 2 | `crates/db` | Crate compartido | common |
| 3 | `crates/events` | Crate compartido | common |
| 4 | `crates/proto` | Crate compartido | — |
| 5 | `crates/bedrock-client` | Crate compartido | common |
| 6 | `apps/credential-manager` | Servicio hoja | db, proto, common |
| 7 | `apps/api-service` | Servicio hoja | db, proto, common |
| 8 | `apps/auth-service` | Servicio hoja (incluye perfil) | db, proto, common |
| 9 | `apps/key-services` | Servicio intermedio | db, proto, events, common |
| 10 | `apps/subscription-service` | Servicio intermedio | db, proto, events, common |
| 11 | `apps/payment-services` | Servicio intermedio | db, proto, events, common |
| 12 | `apps/model-router` | Núcleo hot path | db, proto, common (gRPC→credential-manager) |
| 13 | `apps/api-gateway` | Núcleo hot path | proto, bedrock-client, common (gRPC→key-services, model-router, billing-service) |
| 14 | `apps/billing-service` | Consumidor async | db, proto, events, common |
| 15 | `apps/usage-service` | Consumidor async | db, proto, events, common |
| 16 | `apps/invoice-service` | Consumidor async | db, proto, events, common |
| 17 | `apps/file-service` | Servicio de archivos y RAG | db, proto, events, common |
| 18 | `apps/s3-gateway` | API S3 compatible sobre R2 | proto, common |
| 19 | `apps/customer-bff` | BFF | db, proto, common |
| 20 | `apps/admin-bff` | BFF | db, proto, common |

**Total operativo: 20 artefactos (5 crates + 15 apps, sin contar
`init-db-helper`).** Se eliminó `user-service`; el perfil vive en
`auth-service`. `file-service` y `s3-gateway` son bounded contexts actuales.

---

## Cambios respecto a los documentos originales

| Documento | Qué cambia | Motivo |
|-----------|-----------|--------|
| `01-arquitectura` | Se elimina `user-service` | Sin organizaciones ni equipos |
| `01-arquitectura` | `customer-bff` no agrega datos de org | Sin orgs, el dashboard es solo del usuario |
| `02-schema-base-datos` | Se elimina schema `identity` (`organizations`, `org_members`) | Sin multi-tenant |
| `02-schema-base-datos` | `org_id` → `user_id` en `api_keys`, `wallets`, `subscriptions`, `invoices`, `usage_events`, `payment_methods`, `webhook_endpoints` | La propiedad es del usuario, no de una org |
| `02-schema-base-datos` | Se elimina `user_profiles` | El perfil se absorbe en `auth.users` |

---

## Estado actual del workspace

- PostgreSQL se ejecuta en un contenedor compartido, pero cada servicio usa su
  propia base de datos (`auth_db`, `catalog_db`, `files_db`, `usage_db`, etc.);
  la arquitectura actual no usa schemas compartidos entre servicios.
- `file-service` es dueño de archivos, carpetas, cuotas, ingestión, referencias
  canónicas y uso vectorial. `customer-bff` solo orquesta HTTP y
  `s3-gateway` expone la API S3-compatible sobre Cloudflare R2.
- Qdrant se usa como almacenamiento vectorial; el aislamiento se impone por
  `user_id` desde backend y la resolución de referencias se valida en
  PostgreSQL.
- Vault administra secretos críticos, incluidos credenciales de bases de
  datos, R2, Qdrant, OpenRouter, JWT y material TLS/mTLS. Los `.env` no son la
  fuente de secretos de producción.
- NATS JetStream proporciona Outbox, consumidores durables, ACK, reintentos y
  DLQ. OTEL Collector, Tempo, Loki, Alloy, Prometheus y Grafana forman la
  observabilidad operativa.
- `admin-bff` y `admin-plataform-dashboard` son componentes actuales, al igual
  que `s3-gateway`; cualquier referencia histórica que los omita está obsoleta.

---

## Estructura común de carpetas para cada servicio

```
apps/<nombre-servicio>/
├── Cargo.toml
├── migrations/
│   └── <YYYYMMDDHHMMSS>_<descripcion>.sql
└── src/
    ├── main.rs                    # composition root: wiring de dependencias
    ├── domain/
    │   ├── mod.rs
    │   ├── entities/              # entidades y value objects
    │   ├── repositories/          # TRAITS de repositorio (puertos)
    │   ├── services/              # lógica de dominio pura
    │   └── errors.rs              # errores de dominio (thiserror)
    ├── application/
    │   ├── mod.rs
    │   ├── commands/              # casos de uso que mutan estado
    │   ├── queries/               # casos de uso de solo lectura
    │   └── ports/                 # traits de dependencias externas
    ├── infrastructure/
    │   ├── mod.rs
    │   ├── persistence/           # implementación sqlx de repositorios
    │   ├── messaging/             # implementación NATS de EventPublisher
    │   ├── cache/                 # implementación Redis
    │   └── clients/               # clientes gRPC, AWS SDK
    └── interfaces/
        ├── mod.rs
        └── grpc/                  # implementación de servicios .proto
```

Los BFF añaden además `interfaces/http/` para handlers Axum, DTOs y mappers de
la frontera frontend → BFF. `api-gateway` y `s3-gateway` también implementan
HTTP público; los demás servicios solo exponen HTTP operativo para salud y
métricas cuando corresponde.

---

## 1. `crates/common`

**Propósito:** Tipos transversales compartidos por todos los servicios. Sin lógica de dominio.

### Structs

| Struct | Campos | Descripción |
|--------|--------|-------------|
| `RequestId` | `Uuid` | UUID v7 propagado end-to-end en logs, gRPC metadata, eventos NATS |
| `Money` | `amount: Decimal`, `currency: String` | Aritmética monetaria segura sin floats |
| `Pagination` | `limit: u32`, `offset: u32` | Paginación estándar (default limit=20) |
| `AppConfig` | `database_url`, `redis_url`, `nats_url`, `grpc_port`, `bff_http_port`, `log_level` | Config base cargada de env vars (`envy`); `bff_http_port` aplica solo a los BFF |
| `ApiKeyPrefix` | `prefix: String` | Formato `sk-live-<8 chars>` — parte visible de una API key |
| `ServiceJwtClaims` | `issuer: String`, `subject: String`, `exp: u64` | Claims para autenticación servicio-a-servicio |

### Enums

| Enum | Variantes |
|------|-----------|
| `DomainError` | `NotFound`, `Unauthorized`, `Forbidden`, `Conflict`, `Validation(String)`, `RateLimited`, `PaymentRequired`, `ServiceUnavailable`, `Internal(String)` |
| `LogLevel` | `Trace`, `Debug`, `Info`, `Warn`, `Error` |

### Traits

```rust
trait Clock { fn now() -> DateTime<Utc>; }
```

---

## 2. `crates/db`

| Type Alias / Struct | Descripción |
|---------------------|-------------|
| `PgPool` | `Pool<Postgres>` (sqlx) |
| `RedisPool` | `deadpool_redis::Pool` |
| `DbConfig` | `max_connections`, `acquire_timeout_ms`, `idle_timeout_ms` |
| `RedisConfig` | `pool_size`, `timeout_ms` |
| `HealthCheckResult` | `postgres_ok`, `redis_ok`, `nats_ok`, `latency_ms` |

---

## 3. `crates/events`

### Structs (envelope y payloads) — sin `org_id`, con `user_id`

| Struct | Campos clave | Descripción |
|--------|-------------|-------------|
| `EventEnvelope<T>` | `event_id`, `event_type`, `schema_version`, `occurred_at`, `payload: T` | Envoltorio canónico inmutable |
| `UsageRecorded` | `request_id`, `api_key_id`, `user_id`, `model_catalog_id`, `aws_account_id`, `input_tokens`, `output_tokens`, `cached_tokens`, `stream`, `latency_ms`, `ttft_ms`, `status_code`, `error_code` | Al terminar cada request a Bedrock |
| `PaymentSucceeded` | `transaction_id`, `user_id`, `amount`, `currency`, `provider`, `provider_reference` | Recarga o cobro exitoso |
| `PaymentFailed` | `transaction_id`, `user_id`, `amount`, `reason` | Pago fallido |
| `AccountMarkedUnhealthy` | `account_id`, `aws_account_id`, `error_code`, `error_message` | Circuit breaker |
| `AccountRecovered` | `account_id`, `aws_account_id` | Recuperación de cuenta AWS |
| `WalletDebited` | `user_id`, `amount`, `balance_after`, `usage_event_id` | Débito aplicado |
| `WalletLowBalance` | `user_id`, `current_balance`, `threshold` | Alerta saldo bajo |
| `BillingPeriodClosed` | `user_id`, `subscription_id`, `period_start`, `period_end`, `total_usage_cost`, `invoice_id` | Cierre de período |
| `SubscriptionChanged` | `user_id`, `old_plan_id`, `new_plan_id`, `changed_at` | Cambio de plan |
| `ApiKeyRevoked` | `api_key_id`, `user_id`, `revoked_by`, `reason` | Invalidación de cache |
| `RequestFailed` | `request_id`, `api_key_id`, `error_code`, `error_message`, `aws_account_id` | Telemetría |
| `FileIngestRequested` | `file_id`, `user_id`, `object_key`, `mime_type`, `size_bytes`, `strategy_version`, `embedding_model`, `quota_reservation_id` | Confirmación de archivo → worker de embeddings |
| `FileVectorsDeleteRequested` | `file_id`, `user_id`, `reason` | Borrado coordinado de vectores canónicos |

---

## 4. `crates/proto`

### Servicios gRPC

| Servicio | Métodos |
|----------|---------|
| `KeyValidationService` | `ValidateKey(api_key, source_ip) → (is_valid, api_key_id, user_id, status, scopes, overrides)` |
| `AccountSelectionService` | `SelectAccount(model_catalog_id, request_id) → (account_id, aws_account_id, region, model_id)` / `ReportOutcome(account_id, request_id, success, error_code, latency_ms)` |
| `CredentialResolutionService` | `GetCredentials(account_id) → (access_key_id, secret_access_key, session_token, expires_at, region)` / `HealthCheck(account_id) → (is_healthy, consecutive_errors, last_error)` |
| `BillingReservationService` | `ReserveBalance(user_id, estimated_amount, idempotency_key) → (approved, reservation_id, available_balance)` / `ConfirmDebit(reservation_id, user_id, request_id, actual_input, actual_output) → (success, new_balance)` / `ReleaseReservation(reservation_id, user_id)` |
| `PlanResolutionService` | `GetPlanLimits(user_id) → (rpm, tpm, max_concurrent, monthly_quota, billing_mode)` |
| `SubscriptionManagementService` | `CreatePlan`, `UpdatePlan`, `DeactivatePlan`, `GetPlan`, `ListActivePlans`, `CreateSubscription`, `UpgradeSubscription`, `DowngradeSubscription`, `CancelSubscription`, `RenewSubscription`, `MarkSubscriptionPastDue`, `GetActiveSubscription`, `GetSubscription` |
| `CatalogService` | `ListModels(filters)`, `GetModel(id)`, `GetModelByPublicName(name)`, `GetCurrentPricing(model_id)`, `ListProviders` |
| `AuthService` | `RegisterUser`, `VerifyEmail`, `Login`, `RefreshAccessToken`, `Logout`, `GetUser`, `UpdateProfile`, `ValidateAccessToken`, `ManageMfa` |

### API pública HTTP del `api-gateway`

| Endpoint | Caso de uso |
|----------|-------------|
| `POST /v1/chat/completions` | `ProxyChatCompletion`, `ProxyChatCompletionStream` |
| `POST /v1/embeddings` | `ProxyEmbeddings` |
| `GET /health` | `HealthCheck` |

---

## 5. `crates/bedrock-client`

Wrapper sobre `aws-sdk-bedrockruntime`. Sin entidades propias.

| Structs | Funciones |
|---------|-----------|
| `ConverseRequest`, `ConverseResponse`, `ConverseStreamChunk`, `TokenUsage`, `Message`, `ContentBlock` | `converse(credentials, request) → Result<ConverseResponse>`, `converse_stream(credentials, request) → Result<impl Stream>`, `extract_token_usage(response) → TokenUsage`, `map_bedrock_error(error) → DomainError` |

---

## 6. `apps/credential-manager`

**Bounded context:** Pool de credenciales AWS. Base de datos `credentials_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `AwsCredential` | `credentials.aws_credential_pool` | `auth_method = sts_assume_role` → `role_arn` requerido. `auth_method = static_keys` → `access_key_encrypted` requerido. `max_tpm > 0`, `max_rpm > 0` |
| `AwsAccountModel` | `credentials.aws_account_models` | Referencia lógica a `catalog.model_catalog` |

### Value Objects

| VO | Campos |
|----|--------|
| `StsSessionToken` | `access_key_id`, `secret_access_key`, `session_token`, `expires_at`, `region` |
| `AccountHealth` | `account_id`, `is_healthy`, `last_check_at`, `last_error_code`, `consecutive_errors` |
| `AuthMethod` | Enum: `StsAssumeRole { role_arn, external_id }`, `StaticKeys { access_key_encrypted, secret_key_encrypted }` |

### Errores

```rust
enum CredentialManagerError { AccountNotFound, AccountNotActive, AccountAlreadyExists,
    StsAssumeRoleFailed, KmsDecryptionFailed, InvalidAuthMethod,
    NoHealthyAccountsForModel, CredentialExpired, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `RegisterAwsAccount` | Alta de cuenta AWS en el pool |
| `RemoveAwsAccount` | Baja lógica (`is_active = false`) |
| `UpdateAccountMetadata` | Actualizar max_tpm, priority_weight, etc. |
| `AddModelToAccount` | Asociar modelo a cuenta |
| `RemoveModelFromAccount` | Desasociar modelo |
| `MarkUnhealthy` | Marcar no saludable + evento |
| `MarkRecovered` | Restaurar salud + evento |
| `RunHealthChecks` | Health check periódico |

| Query | Descripción |
|-------|-------------|
| `GetCredentials` | **gRPC hot path** — credenciales STS vigentes |
| `GetAccount` | Cuenta por ID |
| `ListAccounts` | Listado admin |
| `ListHealthyAccountsForModel` | Cuentas sanas que soportan un modelo, ordenadas por prioridad |
| `ListModelsForAccount` | Modelos que soporta una cuenta |

### Traits

```rust
trait AwsCredentialRepository { save, find_by_id, find_by_aws_account_id,
    find_healthy_accounts_for_model, find_all_active, update_health, soft_delete }
trait AwsAccountModelRepository { save, delete, find_models_by_account }
trait StsTokenProvider { assume_role, get_cached_or_refresh, invalidate_cache }
trait KmsDecryptor { decrypt, encrypt }
trait EventPublisher { publish_account_marked_unhealthy, publish_account_recovered }
```

### Interfaz gRPC

- `CredentialResolutionService.GetCredentials`
- `CredentialResolutionService.HealthCheck`

---

## 7. `apps/api-service` (Catalog)

**Bounded context:** Catálogo de modelos y pricing. Base de datos `catalog_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `ModelProvider` | `catalog.model_providers` | `name` único |
| `ModelCatalog` | `catalog.model_catalog` | `public_model_name` único; `context_window > 0`; `max_output_tokens > 0` |
| `ModelPricing` | `catalog.model_pricing` | Precios ≥ 0; solo una fila activa (`effective_to IS NULL`); nunca UPDATE, solo INSERT + cerrar anterior |

### Value Objects

| VO | Campos |
|----|--------|
| `TokenPrice` | `cost_per_1k_input`, `cost_per_1k_output`, `price_per_1k_input`, `price_per_1k_output` |
| `ModelCapabilities` | `supports_streaming`, `supports_tools`, `supports_vision` |
| `PricingMargin` | `aws_cost`, `charged_price`, `multiplier` |

### Errores

```rust
enum CatalogError { ModelNotFound, ProviderNotFound, ModelNameAlreadyExists,
    ProviderNameAlreadyExists, PricingAlreadyActive, NoActivePricingForModel,
    ModelDeprecated, InvalidPricing, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `RegisterModelProvider` | Alta de proveedor |
| `RegisterModel` | Alta de modelo en el catálogo |
| `DeprecateModel` | Marcar modelo como deprecado |
| `SetModelPricing` | Cierra pricing activo + inserta nueva fila |
| `UpdateModelCapabilities` | Actualizar flags de streaming/tools/vision |

| Query | Descripción |
|-------|-------------|
| `ListModels` | Listar con filtros (provider, family, active) |
| `GetModel` | Por ID |
| `GetModelByPublicName` | Por nombre público (usado por api-gateway) |
| `GetCurrentPricing` | Pricing vigente (usado por billing-service) |
| `GetPricingAt` | Pricing en fecha histórica |
| `ListPricingHistory` | Historial completo |
| `ListProviders` | Proveedores activos |

### Traits

```rust
trait ModelProviderRepository { save, find_by_id, find_by_name, find_all_active }
trait ModelCatalogRepository { save, find_by_id, find_by_public_name, find_all, update }
trait ModelPricingRepository { save, close_active_pricing, find_active_by_model, find_at, find_history }
```

### Interfaz gRPC

- `CatalogService.ListModels`, `CatalogService.GetModel`, `CatalogService.GetModelByPublicName`
- `CatalogService.RegisterModelProvider`, `CatalogService.RegisterModel`, `CatalogService.DeprecateModel`
- `CatalogService.SetModelPricing`, `CatalogService.UpdateModelCapabilities`
- `CatalogService.GetCurrentPricing`, `CatalogService.GetPricingAt`, `CatalogService.ListPricingHistory`, `CatalogService.ListProviders`

---

## 8. `apps/auth-service`

**Bounded context:** Identidad de usuarios humanos + perfil básico. Base de datos `auth_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `User` | `auth.users` | `email` único (CITEXT); `status` ∈ {active, locked, disabled}; `failed_login_count` se resetea al login exitoso; campos de perfil incluidos: `display_name`, `avatar_url` |
| `RefreshToken` | `auth.refresh_tokens` | `token_hash` único; `expires_at > created_at`; rotación al refrescar |
| `PasswordReset` | `auth.password_resets` | Expira en 15 min; se consume una sola vez |

### Value Objects

| VO | Descripción |
|----|-------------|
| `Email` | Validado, normalizado a minúsculas |
| `Password` | Min 8 chars, 1 mayúscula, 1 número |
| `PasswordHash` | Argon2id: `m=65536, t=3, p=4` |
| `TokenClaims` | `sub: Uuid`, `email: String`, `exp: u64`, `iat: u64` — JWT de sesión |
| `TOTPSecret` | Cifrado con KMS antes de persistir |
| `LoginAttempt` | `user_id`, `success`, `ip_address` |

### Errores

```rust
enum AuthError { UserNotFound, EmailAlreadyExists, InvalidCredentials, AccountLocked,
    AccountDisabled, EmailNotVerified, InvalidRefreshToken, RefreshTokenRevoked,
    PasswordResetTokenExpired, PasswordResetTokenUsed, MfaRequired, InvalidMfaCode,
    MfaAlreadyEnabled, MfaNotEnabled, PasswordTooWeak, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `RegisterUser` | Registro con email + password. Hash Argon2id. Envía verificación. |
| `VerifyEmail` | Verifica token, marca `email_verified_at` |
| `Login` | Credenciales → JWT + refresh token. Si falla → incrementa contador. Si supera umbral → bloqueo. |
| `RefreshAccessToken` | Rota refresh token (invalida viejo, emite nuevo par) |
| `Logout` | Revoca refresh token específico |
| `LogoutAll` | Revoca todos los refresh tokens del usuario |
| `ChangePassword` | Verifica actual, cambia, revoca todos los tokens |
| `RequestPasswordReset` | Token 15 min TTL. Siempre responde OK (anti-enumeración). |
| `ResetPassword` | Consume token, cambia password |
| `EnableMfa` | Genera TOTP, cifra con KMS |
| `ConfirmMfa` | Verifica código, activa MFA |
| `DisableMfa` | Verifica código, desactiva |
| `UpdateProfile` | Cambiar `display_name`, `avatar_url` |
| `UnlockAccount` | Admin: desbloquea |
| `DisableUser` | Admin: desactiva cuenta |

| Query | Descripción |
|-------|-------------|
| `GetUser` | Por ID |
| `GetUserByEmail` | Búsqueda por email (login) |
| `ValidateAccessToken` | Validar firma, expiración, estado |
| `ListUsers` | Admin: listar |

### Traits

```rust
trait UserRepository { save, find_by_id, find_by_email, update, find_all,
    increment_failed_login, reset_failed_login, lock_account }
trait RefreshTokenRepository { save, find_by_hash, revoke, revoke_all_for_user, cleanup_expired }
trait PasswordResetRepository { save, find_by_hash, mark_used, cleanup_expired }
trait PasswordHasher { hash, verify }
trait TokenSigner { sign, verify }
trait TotpService { generate_secret, verify }
trait EmailSender { send_verification_email, send_password_reset }
```

### Interfaz gRPC

- `AuthService.RegisterUser`, `AuthService.VerifyEmail`, `AuthService.Login`, `AuthService.RefreshAccessToken`, `AuthService.Logout`, `AuthService.LogoutAll`
- `AuthService.ChangePassword`, `AuthService.RequestPasswordReset`, `AuthService.ResetPassword`
- `AuthService.EnableMfa`, `AuthService.ConfirmMfa`, `AuthService.DisableMfa`
- `AuthService.GetUser`, `AuthService.UpdateProfile`, `AuthService.ValidateAccessToken`, `AuthService.ListUsers`, `AuthService.UnlockAccount`, `AuthService.DisableUser`

---

## 9. `apps/key-services`

**Bounded context:** Ciclo de vida de API keys de tráfico. Base de datos `keys_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `ApiKey` | `keys.api_keys` | `key_hash` único; `key_prefix` formato `sk-live-<8 chars>`; `status` ∈ {active, revoked, expired}; 256 bits de entropía; pertenece a `user_id` directamente. Incluye `last_used_ip`, `revoked_at` y `revoked_reason`; no almacena `plan_id`. |

### Value Objects

| VO | Descripción |
|----|-------------|
| `ApiKeyPlaintext` | `sk-live-<prefix><32 bytes random base64url>` — se expone solo al crear |
| `ApiKeyHash` | HMAC-SHA256(plaintext, server_pepper) — lo que se persiste |
| `KeyPrefix` | `sk-live-xxxxxxxx` (16 chars visibles) |
| `ApiKeyScope` | Enum: `InferenceWrite`, `InferenceRead`, `KeyManagement`, `BillingRead` |
| `RateLimitOverride` | `rpm`, `tpm`, `concurrent` — override sobre el plan |
| `IpAllowlist` | `ips: Vec<IpAddr>` |

### Errores

```rust
enum KeyServiceError { ApiKeyNotFound, ApiKeyRevoked, ApiKeyExpired, KeyAlreadyRevoked,
    MaxKeysPerUserReached, InvalidScope, IpNotAllowed, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `CreateApiKey` | Genera key 256 bits, guarda hash. Devuelve plaintext UNA vez. |
| `RevokeApiKey` | `status = revoked` + evento `ApiKeyRevoked` para invalidar cache. |
| `RotateApiKey` | Revoca vieja + crea nueva (mismos scopes) |
| `UpdateKeyMetadata` | Cambiar name, allowed_ips, rate_limit_override |

| Query | Descripción |
|-------|-------------|
| `ValidateKey` | **gRPC hot path.** Hash → entidad. Cache L1 (memoria, 60s) + L2 (Redis, 5 min). Devuelve identidad, estado, scopes y overrides; el plan/límites se resuelven después con `subscription-service` mediante `PlanResolutionService`. |
| `ListApiKeys` | Keys del usuario (solo prefijos, nunca hash) |
| `GetApiKey` | Metadata de una key |

### Traits

```rust
trait ApiKeyRepository { save, find_by_id, find_by_hash, find_by_user, count_active_by_user,
    revoke, update_last_used, update }
trait ApiKeyCache { get, set, invalidate }
trait KeyGenerator { generate → (ApiKeyPlaintext, ApiKeyHash, KeyPrefix) }
trait EventPublisher { publish_api_key_revoked }
```

### Interfaz gRPC

- `KeyValidationService.ValidateKey`

---

## 10. `apps/subscription-service`

**Bounded context:** Planes y suscripciones por usuario. Base de datos `subscription_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `Plan` | `subscription.plans` | `slug` único; `billing_mode` ∈ {prepaid, postpaid}; `rpm > 0`, `tpm > 0`; `price_multiplier ≥ 1.0` |
| `Subscription` | `subscription.subscriptions` | `user_id` (no `org_id`); `status` ∈ {trialing, active, past_due, canceled}; `billing_cycle` ∈ {monthly, annual}; un usuario solo puede tener una suscripción vigente (`trialing`, `active` o `past_due`) |

### Value Objects

| VO | Descripción |
|----|-------------|
| `PlanLimits` | `rpm`, `tpm`, `max_concurrent`, `monthly_token_quota`, `billing_mode` |
| `BillingCycle` | Enum: `Monthly`, `Annual` |
| `SubscriptionStatus` | Enum: `Trialing`, `Active`, `PastDue`, `Canceled` |

### Errores

```rust
enum SubscriptionError { PlanNotFound, PlanNotActive, SubscriptionNotFound,
    SubscriptionNotActive, UserAlreadySubscribed, CannotDowngradePrepaidToPostpaid,
    UpgradeRequiresPayment, InvalidBillingCycle, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `CreatePlan` | Admin: nuevo plan |
| `UpdatePlan` | Admin: actualizar límites/features |
| `DeactivatePlan` | Admin: desactivar |
| `CreateSubscription` | Suscribir usuario a plan. Si prepago → requiere recarga inicial. |
| `UpgradeSubscription` | Cambiar a plan superior (con prorrateo si aplica) |
| `DowngradeSubscription` | Cambiar a plan inferior (efectivo al fin de ciclo) |
| `CancelSubscription` | Cancelar (ahora o al fin de período) |
| `RenewSubscription` | Renovar/reactivar una suscripción vigente, incluyendo `past_due` |
| `SetSubscriptionPastDue` | Admin: marcar como moroso |

| Query | Descripción |
|-------|-------------|
| `GetPlan` | Por ID |
| `ListActivePlans` | Planes disponibles |
| `GetPlanLimits` | **gRPC.** Límites vigentes del usuario (cacheado en Redis) |
| `GetActiveSubscription` | Suscripción activa del usuario |
| `GetSubscription` | Suscripción por ID |

### Interfaz gRPC

- `PlanResolutionService.GetPlanLimits`
- `SubscriptionManagementService.CreatePlan`
- `SubscriptionManagementService.UpdatePlan`
- `SubscriptionManagementService.DeactivatePlan`
- `SubscriptionManagementService.GetPlan`
- `SubscriptionManagementService.ListActivePlans`
- `SubscriptionManagementService.CreateSubscription`
- `SubscriptionManagementService.UpgradeSubscription`
- `SubscriptionManagementService.DowngradeSubscription`
- `SubscriptionManagementService.CancelSubscription`
- `SubscriptionManagementService.RenewSubscription`
- `SubscriptionManagementService.MarkSubscriptionPastDue`
- `SubscriptionManagementService.GetActiveSubscription`
- `SubscriptionManagementService.GetSubscription`

### Traits

```rust
trait PlanRepository { save, find_by_id, find_all_active, update }
trait SubscriptionRepository { save, find_by_id, find_active_by_user, find_current_by_user, update }
trait PlanLimitsCache { get, set, invalidate }
trait EventPublisher { publish_subscription_changed }
```

---

## 11. `apps/payment-services`

**Bounded context:** Integración con proveedores de pago. Base de datos `payments_db`.

Proveedor activo: PayPal. Wompi, Payoneer y PayU no forman parte del despliegue actual.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `PaymentMethod` | `payments.payment_methods` | Pertenece a `user_id`. Nunca almacena PAN, solo referencias tokenizadas (`provider_payment_method_id`) de PayPal. |
| `PaymentProviderEvent` | `payments.payment_provider_events` | `provider_event_id` único (deduplicación de webhooks) |
| `PaymentTransaction` | `payments.payment_transactions` | Registro explícito del pago/recarga/cargo; `provider + provider_reference` único; `provider + idempotency_key` único; status ∈ {pending, requires_approval, succeeded, failed, refunded}. |

### Value Objects

| VO | Descripción |
|----|-------------|
| `CardInfo` | `brand`, `last4`, `exp_month`, `exp_year` |
| `PaymentOrder` | `amount`, `currency`, `provider_reference`, `approval_url`, `status` |
| `CapturedPayment` | `provider_reference`, `order_id`, `amount`, `currency`, `status` |
| `VaultSetup` | `setup_token_id`, `approval_url`, `status` |
| `PaymentToken` | `payment_token_id`, `payer_id`, `email` |
| `WebhookSignature` | Headers firmados del proveedor; en PayPal: `PAYPAL-TRANSMISSION-*`, `PAYPAL-CERT-URL`, `PAYPAL-AUTH-ALGO` |

### Errores

```rust
enum PaymentError { PaymentMethodNotFound, PaymentProviderEventNotFound, PaymentProviderError,
    InvalidWebhookSignature, DuplicateWebhookEvent, PaymentFailed, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `AttachPaymentMethod` | Vincular método de pago al usuario |
| `RemovePaymentMethod` | Desvincular |
| `SetDefaultPaymentMethod` | Cambiar default |
| `CreateTopUp` | Crea orden PayPal para recarga. Idempotente mediante `PayPal-Request-Id`. |
| `CapturePayPalOrder` | Captura una orden aprobada por el comprador y publica `payment.succeeded` una sola vez. |
| `CreatePayPalVaultSetupToken` | Crea setup token de PayPal Vault para guardar PayPal como método de pago sin PAN. |
| `CreatePayPalPaymentToken` | Convierte setup token aprobado en payment token y lo persiste como `PaymentMethod`. |
| `HandlePayPalWebhook` | Verifica firma oficial de PayPal, deduplica, procesa. Publica `payment.succeeded` o `payment.failed`. |

| Query | Descripción |
|-------|-------------|
| `ListPaymentMethods` | Métodos de pago del usuario |
| `GetPaymentMethod` | Detalle (sin secretos) |

### Interfaz gRPC

- `PaymentService.CreateTopUp`
- `PaymentService.CapturePayPalOrder`
- `PaymentService.CreatePayPalVaultSetupToken`
- `PaymentService.CreatePayPalPaymentToken`
- `PaymentService.AttachPaymentMethod`
- `PaymentService.RemovePaymentMethod`
- `PaymentService.SetDefaultPaymentMethod`
- `PaymentService.ListPaymentMethods`
- `PaymentService.GetPaymentMethod`

### Interfaz HTTP externa

- `POST /webhooks/paypal`: webhook público de PayPal. Es frontera externa del proveedor, no comunicación interna entre microservicios.

### Robustez operativa

- Reintentos con backoff para errores temporales de PayPal (`429`/`5xx` y errores de red).
- Outbox transaccional → NATS JetStream con PubAck y reintentos del relay.
- Audit log en acciones sensibles: top-up creado/capturado, webhook procesado y cambios de métodos de pago.
- Webhook HTTP con límite de body, timeout y rate limit por proceso.
- Endpoints operativos HTTP: `/health` y `/metrics` con contadores Prometheus-style para webhooks.
- Script manual de validación PayPal Sandbox: `apps/payment-services/scripts/paypal_sandbox_smoke.sh`.

### Traits

```rust
trait PaymentMethodRepository { save, find_by_id, find_by_user, find_default_by_user, set_default, delete }
trait PaymentProviderEventRepository { save, exists, mark_processed }
trait PaymentTransactionRepository { save, find_by_id, find_by_provider_reference, find_by_idempotency_key, update }
trait PaymentProvider { create_order, capture_order, create_vault_setup_token, create_payment_token, verify_webhook }
trait EventPublisher { publish_payment_succeeded, publish_payment_failed }
trait AuditLogger { log }
```

---

## 12. `apps/model-router`

**Bounded context:** Enrutamiento y balanceo de carga interno. Stateless — estado en Redis.

### Entidades

Sin entidades persistentes propias.

### Value Objects

| VO | Descripción |
|----|-------------|
| `SelectedAccount` | `account_id`, `aws_account_id`, `region`, `provider_model_id` |
| `AccountLoad` | `account_id`, `active_connections`, `current_tpm`, `max_tpm`, `max_rpm` |
| `CircuitState` | Enum: `Closed`, `Open { until }`, `HalfOpen` |

### Errores

```rust
enum ModelRouterError { NoHealthyAccountForModel, AllAccountsCircuitOpen,
    CredentialResolutionFailed, AccountNotFound, RedisUnavailable }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `SelectAccount` | **Implementado gRPC hot path.** Least-connections ponderado por active connections + headroom RPM/TPM + circuit breaker. Incrementa carga activa/RPM/TPM estimada en Redis. |
| `ReportOutcome` | **Implementado gRPC hot path.** Decrementa carga activa, registra tokens reales, evalúa circuit breaker y publica eventos de salud. |
| `ResetCircuitBreaker` | **Implementado gRPC admin.** Fuerza cierre del circuito. |

| Query | Descripción |
|-------|-------------|
| `GetAccountLoad` | **Implementado gRPC admin.** Carga actual de una cuenta. |
| `ListAccountLoads` | **Implementado gRPC admin.** Carga de todas las cuentas sanas para un modelo. |
| `GetCircuitStatus` | **Implementado gRPC admin.** Estado del circuit breaker. |

### Puertos

```rust
trait CredentialResolver { get_credentials, list_healthy_accounts }
trait LoadTracker { increment_active, decrement_active, record_tpm, get_load, get_all_loads }
trait CircuitBreaker { is_open, record_success, record_failure, force_close }
trait EventPublisher { publish_account_marked_unhealthy, publish_account_recovered }
```

### Interfaz gRPC

- `AccountSelectionService.SelectAccount`
- `AccountSelectionService.ReportOutcome`
- `AccountSelectionService.ResetCircuitBreaker`
- `AccountSelectionService.GetCircuitStatus`
- `AccountSelectionService.GetAccountLoad`
- `AccountSelectionService.ListAccountLoads`

### Conexiones internas

- Consume `credential-manager` por gRPC (`ListHealthyAccountsForModel`) con JWT de servicio.
- Consume `api-service` por gRPC (`CatalogService.GetModel`) con JWT de servicio para resolver `provider_model_id`.
- Usa Redis para `active_connections`, RPM, TPM estimado/real y estado de circuit breaker.
- Encola `account.marked_unhealthy` y `account.recovered` en `router.event_outbox`.
- HTTP operativo únicamente para infraestructura: `/health` y `/metrics`.

---

## 13. `apps/api-gateway`

**Bounded context:** Edge / Ingress público. Stateless. Capa más externa.

### Entidades

Sin entidades persistentes propias.

### Value Objects

| VO | Descripción |
|----|-------------|
| `ChatCompletionRequest` | `model`, `messages`, `max_tokens?`, `temperature?`, `stream?`, `tools?`, `user?` — compatible OpenAI |
| `ChatCompletionResponse` | `id`, `object`, `created`, `model`, `choices`, `usage` |
| `ChatCompletionChunk` | `id`, `object`, `created`, `model`, `choices` — SSE |
| `EmbeddingRequest` | `model`, `input` |
| `EmbeddingResponse` | `object`, `data`, `model`, `usage` |
| `GatewayError` | `{ code, message, type, param }` — formato error OpenAI |
| `RateLimitState` | `key_id`, `rpm_current`, `rpm_limit`, `tpm_current`, `tpm_limit`, `concurrent`, `concurrent_limit` |

### Errores

```rust
enum GatewayError { InvalidApiKey, ApiKeyRevoked, ApiKeyExpired, RateLimited,
    PaymentRequired, ModelNotAvailable, BedrockError, UpstreamUnavailable,
    InvalidRequestBody, AuthenticationRequired, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `ProxyChatCompletion` | **Implementado HTTP público.** Flujo no-streaming: validar key → plan/rate limit → reserva saldo → seleccionar cuenta → credenciales efímeras → Bedrock Mantle → confirmar débito → reportar outcome → publicar `usage.recorded`/`request.failed` → responder OpenAI-compatible. |
| `ProxyChatCompletionStream` | **Implementado HTTP público SSE.** Mismo hot path que chat no-streaming; al cierre confirma billing, reporta outcome y publica `usage.recorded`. |
| `ProxyEmbeddings` | **Implementado HTTP público.** Mismo hot path que chat no-streaming para `/v1/embeddings`; cobra tokens de entrada y publica `usage.recorded`. |

| Query | Descripción |
|-------|-------------|
| `ListModels` | RPC `InferenceGatewayService.ListModels`, filtrado por plan |
| `HealthCheck` | RPC `InferenceGatewayService.HealthCheck` |

### Puertos (7 traits)

```rust
trait KeyValidator { validate_key, invalidate_cache }
trait RateLimiter { check_limit }
trait BalanceReserver { reserve, confirm_debit, release_reservation }
trait AccountSelector { select_account, report_outcome }
trait BedrockInvoker { converse, converse_stream }
trait EventPublisher { publish_usage_recorded, publish_request_failed }
trait ModelResolver { resolve_model, get_plan_limits }
```

### Interfaz pública

- `POST /v1/chat/completions` no-streaming y streaming SSE implementado.
- `POST /v1/embeddings` implementado.
- `GET /health` implementado.
- HTTP es público hacia SDK/apps/n8n; todas las dependencias internas son gRPC + Protobuf con JWT de servicio.

### Conexiones internas implementadas

- `key-services` gRPC: `ValidateKey`.
- `subscription-service` gRPC: `GetPlanLimits`.
- `api-service` gRPC: `GetModelByPublicName`, `GetCurrentPricing`.
- `billing-service` gRPC: `ReserveBalance`, `ReleaseReservation`, `ConfirmDebit`.
- `model-router` gRPC: `SelectAccount`, `ReportOutcome`.
- `credential-manager` gRPC: `GetCredentials`.
- Redis: RPM/TPM/concurrencia por API key.
- Outbox → NATS JetStream: publica `usage.recorded` y `request.failed`.
- `InferenceGatewayService.ListModels`
- `InferenceGatewayService.HealthCheck`

---

## 14. `apps/billing-service`

**Bounded context:** Medición, cobro y wallet por usuario. Base de datos `billing_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `Wallet` | `billing.wallets` | PK = `user_id`. `balance >= 0`; `reserved_balance >= 0`. Proyección cacheada del ledger. |
| `WalletLedger` | `billing.wallet_ledger` | **Inmutable** (solo INSERT). `entry_type` ∈ {credit, debit, reservation, reservation_release}. `amount > 0`. `idempotency_key` único. |
| `Transaction` | `billing.transactions` | `type` ∈ {topup, charge, refund, adjustment}. `status` ∈ {pending, succeeded, failed, refunded}. |
| `Reservation` | `billing.reservations` | Reserva optimista con `idempotency_key` único; `status` ∈ {pending, confirmed, released, expired}. |
| `IdempotencyKey` | `billing.idempotency_keys` | Expira 24h. `request_hash` detecta reuso incorrecto. |

### Value Objects

| VO | Descripción |
|----|-------------|
| `Balance` | `amount`, `currency` — disponible = `wallet.balance − wallet.reserved_balance` |
| `Reservation` | `id`, `user_id`, `amount`, `idempotency_key`, `created_at` |
| `CostCalculation` | `input_cost`, `output_cost`, `total_cost`, `charged_amount`, `margin_percent` |
| `LedgerEntry` | `entry_type`, `amount`, `balance_after`, `reference_type`, `idempotency_key` |

### Errores

```rust
enum BillingError { WalletNotFound, InsufficientBalance, InsufficientReservedBalance,
    ReservationNotFound, ReservationAlreadyReleased, IdempotencyKeyConflict,
    DuplicateEvent, InvalidPricingForModel, LowBalanceThreshold,
    WalletLedgerIntegrityError, Validation(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `CreateWallet` | Al registrar usuario: wallet balance 0 |
| `TopUpWallet` | Recarga idempotente. Ledger `credit`. |
| `ReserveBalance` | **gRPC hot path.** Reserva optimista pre-Bedrock. |
| `ReleaseReservation` | Libera sin cobrar (error en Bedrock) |
| `ConfirmDebit` | Calcula costo real, aplica débito, libera resto. Emite `WalletDebited`/`WalletLowBalance`. |
| `ProcessUsageRecorded` | **Consumer NATS `usage.recorded`.** Reconciliación async contra la reserva cuyo `idempotency_key` sea `request_id`. |
| `ProcessPaymentSucceeded` | **Consumer NATS `payment.succeeded`.** → `TopUpWallet`. |
| `CloseBillingPeriod` | Postpago: calcula total del ciclo → `BillingPeriodClosed`. |
| `AdjustBalance` | Admin: ajuste manual con auditoría. |
| `ReconcileReservations` | Job: libera reservas huérfanas. |
| `ReconcileWalletFromLedger` | Reconstruye balance desde ledger (verificación integridad). |

| Query | Descripción |
|-------|-------------|
| `GetWallet` | Saldo del usuario |
| `GetWalletBalance` | Disponible (balance − reserved) |
| `GetLedgerEntries` | Historial del ledger |
| `GetTransactions` | Historial de transacciones |
| `CalculateCost` | Estimar costo sin aplicar |

### Interfaz gRPC

- `BillingReservationService.ReserveBalance`
- `BillingReservationService.ReleaseReservation`
- `BillingReservationService.ConfirmDebit`
- `BillingWalletService.GetWallet`
- `BillingWalletService.ListLedger`
- `BillingWalletService.ListTransactions`
- `BillingWalletService.AdjustWallet`
- `BillingWalletService.ReconcileWallet`
- `BillingWalletService.CloseBillingPeriod`

### Traits

```rust
trait WalletRepository { find_by_user, create, update_balance, debit, credit, reserve, release_reservation }
trait WalletLedgerRepository { insert_entry, exists_by_idempotency_key, find_by_user, reconstruct_balance }
trait TransactionRepository { save, find_by_id, find_by_user, update_status }
trait ReservationStore { create, find_by_id, find_orphaned, delete }
trait PricingResolver { get_current_pricing, get_pricing_at }
trait CostCalculator { calculate }
trait EventPublisher { publish_wallet_debited, publish_wallet_low_balance, publish_billing_period_closed }
trait AuditLogger { log_adjustment }
```

### NATS Consumers

- `payment.succeeded` → `ProcessPaymentSucceeded` → `TopUpWallet`.
- `usage.recorded` → `ProcessUsageRecorded` → `ConfirmDebit`.
- Eventos inválidos o fallidos tras cinco intentos van a `EVENTS_DLQ` bajo `dlq.billing_*`.

### Operación

- Job interno de expiración de reservas vencidas.
- Pricing real vía `api-service`/`CatalogService.GetCurrentPricing`.
- Webhooks salientes a `billing.webhook_endpoints` para eventos de wallet.
- Endpoints HTTP operativos: `/health`, `/metrics`.
- Script manual E2E: `apps/billing-service/scripts/e2e_billing_flow.sh`.

| Subject | Handler |
|---------|---------|
| `usage.recorded` | `ProcessUsageEvent` |
| `payment.succeeded` | `ProcessPaymentSucceeded` |

---

## 15. `apps/usage-service`

**Bounded context:** Analítica y reporting. Base de datos `usage_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `UsageEvent` | `usage.usage_events` (particionada por mes) | `request_id` único; `input_tokens >= 0`; `output_tokens >= 0`; referencia a `user_id` |
| `UsageHourly` | `usage.usage_hourly` (vista materializada) | Agregado por `api_key_id`, `user_id`, `model_catalog_id`, hora |
| `UsageDaily` | `usage.usage_daily` (vista materializada) | Agregado por `user_id`, `model_catalog_id`, día |

### Value Objects

| VO | Descripción |
|----|-------------|
| `UsageSummary` | `period`, `total_requests`, `total_input_tokens`, `total_output_tokens`, `total_cost`, `by_model`, `by_key` |
| `TokenStats` | `input`, `output`, `cached`, `total` |
| `LatencyStats` | `min_ms`, `max_ms`, `avg_ms`, `p50_ms`, `p90_ms`, `p99_ms` |
| `ErrorBreakdown` | `error_code`, `count`, `percentage` |

### Errores

```rust
enum UsageError { DuplicateRequestId, InvalidTimeRange, Validation(String), DependencyUnavailable(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `RecordUsageEvent` | **Implementado.** Consumer NATS `usage.recorded`; persiste `usage.usage_events`; idempotente por `request_id` con advisory lock transaccional. |
| `RefreshMaterializedViews` | **Implementado.** Job interno + RPC admin; usa `REFRESH MATERIALIZED VIEW CONCURRENTLY` y ejecución inicial no-concurrent cuando la vista aún no está poblada. |
| `CreateNextMonthPartition` | **Implementado.** Job interno + RPC admin; crea/verifica partición mensual siguiente. |
| `ArchiveOldPartitions` | Pendiente para ciclo de retención/archivado a S3/Glacier. |

| Query | Descripción |
|-------|-------------|
| `GetUsageByUser` | **Implementado gRPC.** Uso de un usuario en rango de fechas. |
| `GetUsageByApiKey` | **Implementado gRPC.** Uso de una key específica. |
| `GetUsageByModel` | **Implementado gRPC.** Uso de un modelo para admin/reporting. |
| `GetCurrentMonthUsage` | **Implementado gRPC.** Resumen del mes actual para dashboard. |
| `GetTopModels` | **Implementado gRPC.** Modelos más usados. |
| `GetErrorRate` | **Implementado gRPC.** Tasa de error y buckets por `error_code`/status. |
| `GetLatencyStats` | **Implementado gRPC.** `min`, `avg`, `p50`, `p95`, `p99`, `max`. |

### Traits

```rust
trait UsageEventRepository { save, exists_by_request_id, find_by_user, find_by_api_key,
    aggregate_by_user, aggregate_by_model, get_error_rate, get_latency_stats }
trait MaterializedViewRefresher { refresh_hourly, refresh_daily }
trait PartitionManager { create_partition, ensure_next_month_partition, drop_partition, archive_to_s3 }
```

### NATS Consumers

| Subject | Handler |
|---------|---------|
| `usage.recorded` | `RecordUsageEvent` → `usage.usage_events`; eventos inválidos/fallidos → `dlq.usage_service.usage.recorded` |

### Contratos expuestos

- `crates/proto/proto/usage.proto`
- `UsageReportingService`
- Autenticación gRPC servicio-a-servicio con `SERVICE_JWT_SECRET`, `USAGE_SERVICE_ALLOWED_ISSUERS` y `USAGE_SERVICE_ALLOWED_SUBJECTS`.
- RPCs operativos/admin (`RefreshMaterializedViews`, `CreateNextMonthPartition`) restringidos adicionalmente por `USAGE_SERVICE_ADMIN_SUBJECTS`.
- HTTP operativo únicamente para infraestructura: `/health` y `/metrics`.

---

## 16. `apps/invoice-service`

**Bounded context:** Facturación documental. Base de datos `invoicing_db`.

### Entidades

| Entidad | Tabla | Invariantes |
|---------|-------|-------------|
| `Invoice` | `invoicing.invoices` | Pertenece a `user_id`. `invoice_number` secuencial único. `total = subtotal + tax`. `status` ∈ {draft, open, paid, void, uncollectible}. |
| `InvoiceLineItem` | `invoicing.invoice_line_items` | `amount = quantity × unit_price` |
| `InvoiceSequence` | `invoicing.invoice_sequences` | Numeración correlativa por período `YYYYMM`. |

### Value Objects

| VO | Descripción |
|----|-------------|
| `InvoicePeriod` | `start`, `end` |
| `InvoiceSummary` | `invoice_id`, `invoice_number`, `total`, `status`, `period` |
| `PdfDocument` | `url`, `bytes` |

### Errores

```rust
enum InvoiceError { InvoiceNotFound, InvoiceNumberAlreadyExists, InvoiceNotDraft,
    InvoiceAlreadyPaid, InvoiceVoided, InvalidPeriod, PdfGenerationFailed,
    PdfStorageFailed, Validation(String), DependencyUnavailable(String) }
```

### Casos de uso

| Comando | Descripción |
|---------|-------------|
| `GenerateInvoice` | **Implementado dentro de `ProcessBillingPeriodClosed`.** Crea invoice `open` idempotente con número correlativo. |
| `OpenInvoice` | **Implementado gRPC admin.** `draft` → `open`. |
| `MarkAsPaid` | **Implementado gRPC admin.** `open` → `paid`. |
| `VoidInvoice` | **Implementado gRPC admin.** Anular desde `draft`. |
| `MarkAsUncollectible` | **Implementado gRPC admin.** `open` → `uncollectible`. |
| `GeneratePdf` | **Implementado.** Genera PDF real y lo almacena en Cloudflare R2, guardando `pdf_url` con esquema `r2://`. |
| `ProcessBillingPeriodClosed` | **Implementado.** Consumer NATS `billing.period_closed`; genera invoice con line items por modelo desde `usage.usage_events`. |
| `AuditInvoiceActions` | **Implementado.** Registra `invoice.generated`, `invoice.opened`, `invoice.paid`, `invoice.voided` e `invoice.uncollectible` en `audit.audit_logs`. |

| Query | Descripción |
|-------|-------------|
| `GetInvoice` | **Implementado gRPC.** Detalle con line items. |
| `ListInvoicesByUser` | **Implementado gRPC.** Listado del usuario. |
| `GetInvoicePdf` | **Implementado gRPC.** Descarga bytes PDF desde Cloudflare R2. |
| `GetNextInvoiceNumber` | **Implementado gRPC admin.** Próximo correlativo. |

### Traits

```rust
trait InvoiceRepository { save, find_by_id, find_by_number, find_by_user, update_status, update_pdf_url, get_next_sequence_number }
trait InvoiceLineItemRepository { save_batch, find_by_invoice }
trait PdfGenerator { generate_invoice_pdf }
trait PdfStorage { store, retrieve }
trait UsageAggregator { get_usage_for_period_from_usage_events }
```

### NATS Consumers

| Subject | Handler |
|---------|---------|
| `billing.period_closed` | `ProcessBillingPeriodClosed`; errores inválidos/fallidos → `dlq.invoice_service.billing.period.closed` |

### Contratos expuestos

- `crates/proto/proto/invoice.proto`
- `InvoiceService`
- Autenticación gRPC servicio-a-servicio con `SERVICE_JWT_SECRET`, `INVOICE_SERVICE_ALLOWED_ISSUERS` y `INVOICE_SERVICE_ALLOWED_SUBJECTS`.
- RPCs administrativos de estado y correlativo restringidos por `INVOICE_SERVICE_ADMIN_SUBJECTS`.
- PDFs almacenados en Cloudflare R2 con `INVOICE_R2_ACCOUNT_ID`, `INVOICE_R2_ENDPOINT_URL`, `INVOICE_R2_ACCESS_KEY_ID`, `INVOICE_R2_SECRET_ACCESS_KEY`, `INVOICE_R2_BUCKET` e `INVOICE_R2_PREFIX`.
- HTTP operativo únicamente para infraestructura: `/health` y `/metrics`.

---

## 17. `apps/file-service`

**Bounded context:** Archivos, cuotas, ingestión, embeddings y referencias
vectoriales. Base de datos `files_db`.

### Responsabilidades

- Crear, listar, renombrar, mover, confirmar y borrar archivos y carpetas con
  ownership obligatorio por `user_id`.
- Reservar cuotas atómicamente y aplicar límites de plan antes de descargar R2
  o llamar a OpenRouter.
- Generar URLs firmadas de Cloudflare R2 y confirmar tamaño, MIME real y hash.
- Ejecutar Outbox → NATS JetStream → extracción → chunking → embeddings
  OpenRouter → Qdrant, con idempotencia, deduplicación, ACK, reintentos y DLQ.
- Resolver referencias autorizadas en PostgreSQL y coordinar el borrado de
  vectores canónicos.

### Interfaz gRPC

`FileService` incluye archivos, carpetas, uso de almacenamiento/vectorial,
URLs firmadas, búsqueda RAG, reindexado y borrado coordinado.

---

## 18. `apps/s3-gateway`

**Bounded context:** API S3-compatible de Pro Plus sobre Cloudflare R2. No usa
AWS S3.

### Responsabilidades

- Validar AWS SigV4 y scopes `files:read`, `files:write` y `files:delete`.
- Delegar ownership, cuotas y metadatos al `file-service`.
- Implementar objetos, listado, streaming, rangos `206`, multipart y hash en
  streaming.
- Forzar el namespace privado derivado de la identidad autenticada; el cliente
  no puede enviar `user_id`, colección ni `qdrant_point_id`.

### Exposición

HTTP/S3 en el dominio de almacenamiento configurado (`storage.jnmtortas.com`)
y puerto local `4002`.

---

## 19. `apps/customer-bff`

**Bounded context:** Backend-for-frontend del dashboard de cliente. Sin schema propio.

### Responsabilidades

- Agrega llamadas **gRPC + Protobuf** a: `auth-service`, `key-services`, `subscription-service`, `invoice-service`, `usage-service`, `payment-services`, `billing-service` y `file-service`.
- Cookie de sesión: `HttpOnly + Secure + SameSite=Strict`
- Valida JWT en cada request
- Sin lógica de negocio propia — solo agregación y mapeo de DTOs

### Value Objects

| VO | Descripción |
|----|-------------|
| `DashboardSummary` | `current_plan`, `balance`, `monthly_usage`, `active_keys`, `recent_invoices` |
| `SessionCookie` | `jwt`, `http_only`, `secure`, `same_site`, `max_age` |

### Interfaz HTTP/JSON — frontend → `customer-bff`

| Método | Ruta | Fuente gRPC |
|--------|------|-------------|
| `GET` | `/api/dashboard` | Agregación gRPC: plan + balance + uso + keys + facturas |
| `GET` | `/api/dashboard/usage` | `usage-service` |
| `GET` | `/api/dashboard/invoices` | `invoice-service` |
| `GET` | `/api/dashboard/invoices/{id}/pdf` | `invoice-service` |
| `GET` | `/api/keys` | `key-services` |
| `POST` | `/api/keys` | `key-services` |
| `DELETE` | `/api/keys/{id}` | `key-services` |
| `GET` | `/api/billing/balance` | `billing-service` |
| `GET` | `/api/billing/transactions` | `billing-service` |
| `GET` | `/api/billing/ledger` | `billing-service` |
| `POST` | `/api/billing/top-up` | `payment-services` |
| `GET` | `/api/payment-methods` | `payment-services` |
| `POST` | `/api/payment-methods` | `payment-services` |
| `DELETE` | `/api/payment-methods/{id}` | `payment-services` |
| `GET` | `/api/subscription` | `subscription-service` |
| `POST` | `/api/subscription/upgrade` | `subscription-service` |
| `POST` | `/api/subscription/cancel` | `subscription-service` |
| `GET` | `/api/profile` | `auth-service` |
| `PATCH` | `/api/profile` | `auth-service` |
| `POST` | `/api/profile/change-password` | `auth-service` |
| `POST` | `/api/mfa/enable` | `auth-service` |
| `POST` | `/api/mfa/confirm` | `auth-service` |
| `POST` | `/api/mfa/disable` | `auth-service` |
| `GET/POST/PATCH/DELETE` | `/api/v1/files`, `/api/v1/folders` | `file-service` |
| `POST` | `/api/v1/vectors/search` | `file-service` |
| `GET` | `/api/v1/vectors/usage` | `file-service` |
| `POST` | `/api/v1/files/{id}/vectors/reindex` | `file-service` |

---

## 20. `apps/admin-bff`

**Bounded context:** Backend-for-frontend del panel admin. Sin schema propio.

### Seguridad

- Dominio separado del customer-bff
- MFA obligatorio para operadores
- Rol `admin` verificado en cada endpoint
- Auditoría obligatoria en acciones sensibles

### Fuentes de datos

- Cada fuente de datos se consume mediante **gRPC + Protobuf**; `admin-bff` no usa HTTP para comunicarse con microservicios.

### Interfaz HTTP/JSON — frontend → `admin-bff`

| Método | Ruta | Fuente gRPC |
|--------|------|-------------|
| `GET` | `/admin/dashboard` | Salud del sistema, métricas globales |
| `GET` | `/admin/users` | `auth-service` |
| `POST` | `/admin/users/{id}/disable` | `auth-service` |
| `POST` | `/admin/users/{id}/unlock` | `auth-service` |
| `GET` | `/admin/aws-accounts` | `credential-manager` |
| `POST` | `/admin/aws-accounts` | `credential-manager` |
| `PATCH` | `/admin/aws-accounts/{id}` | `credential-manager` |
| `DELETE` | `/admin/aws-accounts/{id}` | `credential-manager` |
| `POST` | `/admin/aws-accounts/{id}/add-model` | `credential-manager` |
| `DELETE` | `/admin/aws-accounts/{id}/models/{mid}` | `credential-manager` |
| `GET` | `/admin/models` | `api-service` |
| `POST` | `/admin/models` | `api-service` |
| `PATCH` | `/admin/models/{id}` | `api-service` |
| `POST` | `/admin/models/{id}/pricing` | `api-service` |
| `DELETE` | `/admin/models/{id}` | `api-service` |
| `GET` | `/admin/plans` | `subscription-service` |
| `POST` | `/admin/plans` | `subscription-service` |
| `PATCH` | `/admin/plans/{id}` | `subscription-service` |
| `DELETE` | `/admin/plans/{id}` | `subscription-service` |
| `GET` | `/admin/subscriptions` | `subscription-service` |
| `POST` | `/admin/subscriptions/{id}/past-due` | `subscription-service` |
| `GET` | `/admin/billing/transactions` | `billing-service` |
| `POST` | `/admin/billing/adjust` | `billing-service` |
| `POST` | `/admin/billing/reconcile/{user_id}` | `billing-service` |
| `GET` | `/admin/usage/global` | `usage-service` |
| `GET` | `/admin/usage/errors` | `usage-service` |
| `POST` | `/admin/model-router/reset-circuit/{id}` | `model-router` |
| `GET` | `/api/v1/admin/storage/users/{user_id}` | `file-service` + JetStream DLQ |

---

## Resumen global

| Servicio | Entidades | VOs | Commands | Queries | Traits repo | Puertos |
|----------|----------|-----|----------|---------|-------------|---------|
| common | — | 6 | — | — | — | 1 |
| db | — | 3 | — | — | — | — |
| events | — | 12 | — | — | — | — |
| proto | — | 5 svcs | — | — | — | — |
| bedrock-client | — | 6 | — | — | — | — |
| credential-manager | 2 | 3 | 7 | 5 | 2 | 3 |
| api-service | 3 | 3 | 5 | 7 | 3 | — |
| auth-service | 3 | 6 | 15 | 4 | 3 | 4 |
| key-services | 1 | 5 | 4 | 3 | 2 | 2 |
| subscription-service | 2 | 3 | 9 | 5 | 3 | 1 |
| payment-services | 2 | 3 | 7 | 3 | 2 | 2 |
| model-router | — | 3 | 3 | 3 | — | 4 |
| api-gateway | — | 6 | 3 | 2 | — | 7 |
| billing-service | 4 | 4 | 11 | 5 | 4 | 4 |
| usage-service | 3 | 4 | 4 | 7 | 3 | — |
| invoice-service | 2 | 3 | 7 | 4 | 2 | 3 |
| file-service | files, folders, quotas, ingestion, canonical vectors | — | — | — | — | gRPC, R2, Qdrant, NATS |
| s3-gateway | S3 object/multipart requests | — | — | — | — | SigV4, file-service, R2 |
| customer-bff | — | 2 | — | — | — | HTTP + gRPC |
| admin-bff | — | — | — | — | — | HTTP + gRPC + JetStream |
| **Total** | **Inventario histórico; no usar como métrica de implementación** | — | — | — | — | **Consultar Cargo.toml, proto y migraciones actuales** |

---

## Próximos pasos

1. ✅ Documento reconciliado con el workspace actual
2. Ejecutar la validación operativa pendiente de Fase 3: interoperabilidad S3
3. Ejecutar el gate real de Fase 5 con PostgreSQL, Redis y Qdrant
4. Ejecutar las pruebas live de staging de Fase 9 y registrar sus artefactos
5. Mantener este documento y `01-arquitectura-microservicios.md` actualizados en
   cada cambio de bounded context, transporte, base de datos o proveedor.
