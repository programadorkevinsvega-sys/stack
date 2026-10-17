# AI Gateway — Arquitectura y Responsabilidad de Microservicios

> Reconciliado con el workspace actual el 2026-07-25. Las referencias a
> proveedores, schemas o servicios que no aparecen en "Topología actual" son
> diseño histórico y no representan componentes desplegados.

## 1. Visión general

El sistema es una **capa de reventa sobre proveedores de modelos de IA**. El
chat y los modelos principales usan AWS Bedrock; los embeddings usan el API
OpenAI-compatible de OpenRouter con `qwen/qwen3-embedding-8b`. Expone una API
pública compatible con OpenAI, balancea cuentas AWS, mide consumo, factura y
ofrece dashboards de cliente y administración. Cada usuario es dueño directo
de sus API keys, archivos, vectores, wallet, suscripción y facturación; no hay
`tenant_id`, organizaciones ni equipos.

### Topología actual

| Componente | Responsabilidad actual | Exposición |
|---|---|---|
| `api-gateway` | API pública de inferencia OpenAI-compatible y flujo de reserva/uso | HTTP público |
| `customer-bff` | Sesión y plano de control del dashboard de cliente | HTTP público |
| `admin-bff` | Operación administrativa, usuarios, planes, archivos y DLQ | HTTP restringido |
| `auth-service` | Usuarios, login, MFA, verificación y tokens | gRPC interno |
| `key-services` | API keys, scopes, revocación y rotación | gRPC interno |
| `api-service` | Catálogo, modelos, capacidades y precios | gRPC interno |
| `subscription-service` | Planes, suscripciones, límites, cancelación y dunning | gRPC interno |
| `payment-services` | PayPal checkout, captura, Vault y webhooks | gRPC + webhook PayPal |
| `credential-manager` | Credenciales temporales y salud de cuentas AWS | gRPC interno |
| `model-router` | Selección de cuenta/modelo y circuit breaker | gRPC interno |
| `billing-service` | Wallet, reservas, débitos e invoices de consumo | gRPC + NATS |
| `usage-service` | Eventos de uso, agregados y cuotas analíticas | gRPC + NATS |
| `invoice-service` | Facturas, PDF y almacenamiento R2 | gRPC + NATS |
| `file-service` | Archivos, carpetas, cuotas, ingestión, embeddings y Qdrant | gRPC interno |
| `s3-gateway` | API S3-compatible con SigV4 sobre Cloudflare R2 | HTTP/S3 público |

## 2. Principios de diseño

- **Un microservicio = un Bounded Context DDD.** Cada servicio es dueño exclusivo de sus tablas. Ningún otro servicio escribe directamente en la base de datos de otro; solo lo hace a través de su API (gRPC) o de eventos.
- **Comunicación síncrona (gRPC/tonic)** solo para lo que bloquea la respuesta al usuario final: validar API key, seleccionar cuenta AWS, obtener credenciales STS.
- **Comunicación asíncrona (NATS JetStream)** para todo lo que no debe añadir latencia a la respuesta: billing, agregación de uso, facturación, webhooks salientes, notificaciones.
- **Database-per-service.** Un único contenedor PostgreSQL local aloja bases de datos independientes (`auth_db`, `billing_db`, `usage_db`, `files_db`, etc.) con URLs y migraciones separadas. No se comparten tablas ni schemas de negocio entre servicios.
- **Los BFF son la puerta de entrada de los dashboards.** `api-gateway` atiende la API de inferencia y `s3-gateway` atiende clientes S3 directamente; ninguno es sustituido por un BFF.
- **Idempotencia en todo lo que mueve dinero.** Cualquier operación de billing debe poder reintentarse sin duplicar cargos.
- **Stateless donde se pueda.** `api-gateway` y `model-router` no deben guardar estado que no esté en Redis/Postgres, para escalar horizontalmente sin fricción.

## 3. Flujo de una petición (alto nivel)

```
Cliente (SDK/n8n/app)
   │  POST /v1/chat/completions  (Authorization: Bearer sk-live-...)
   ▼
api-gateway  ──gRPC──► key-services (valida hash de key, identidad, status y scopes; cacheado en Redis)
   │
   ├──gRPC──► usage-service / Redis (rate limit: RPM, TPM, concurrencia)
   │
   ▼
model-router ──gRPC──► credential-manager (credenciales STS vigentes de una cuenta AWS sana
   │                     que soporte el modelo solicitado, elegida por least-connections
   │                     ponderado por headroom de TPM)
   ▼
bedrock-client ──► AWS Bedrock (Converse / ConverseStream)
   │
   ▼
api-gateway devuelve la respuesta (streaming SSE o JSON) al cliente
   │
   └─(async, no bloquea la respuesta)─► NATS JetStream: evento "usage.recorded"
                                             │
                                             ├─► billing-service: calcula costo, actualiza wallet
                                             ├─► usage-service: refresca agregados para el dashboard
                                             └─► OTEL/Prometheus/Loki/Tempo
```

### Flujo actual de archivos y RAG

```
Frontend/SDK S3
   │
   ├─ customer-bff ──gRPC──► file-service ──URL firmada──► Cloudflare R2
   └─ s3-gateway ──SigV4──► file-service ──streaming/multipart──► R2

Confirmación de archivo
   └─► event_outbox ──► NATS JetStream EVENTS
          └─► file-ingestion-worker
                ├─ extrae PDF/DOCX/XLSX/TXT
                ├─ fragmenta con estrategia versionada
                ├─ deduplica por user_id/modelo/versión/content_hash
                ├─ OpenRouter embeddings (Qwen3-Embedding-8B)
                └─ Qdrant + referencias PostgreSQL
```

## 4. Responsabilidad detallada por servicio

### `apps/api-gateway`
- **Bounded context:** Edge / Ingress público.
- **Responsabilidad:** único punto de entrada externo HTTP. Valida body OpenAI-compatible, extrae API key del header `Authorization`, valida en `key-services`, consulta límites de plan en `subscription-service`, resuelve modelo/precio en `api-service`, aplica rate limiting Redis, reserva saldo en `billing-service`, pide cuenta a `model-router`, obtiene credenciales efímeras en `credential-manager`, llama a `bedrock-client` en chat JSON, chat streaming SSE o embeddings, confirma/libera billing, reporta outcome al router y publica `usage.recorded`/`request.failed`.
- **No hace:** no conoce credenciales AWS, no calcula precios, no decide planes, no persiste lógica de negocio. Es la capa más "tonta" y reemplazable del sistema a propósito.
- **Expone:** HTTP público (`/v1/chat/completions`, `/v1/embeddings`, `/health`).
- **Consume:** gRPC de `key-services`, `subscription-service`, `api-service`, `billing-service`, `model-router` y `credential-manager`; Redis para contadores de rate limit.
- **Publica:** `usage.recorded`, `request.failed` a NATS.

### `apps/model-router`
- **Bounded context:** Enrutamiento y balanceo de carga interno.
- **Responsabilidad:** dado un `model_catalog_id` lógico, resuelve `provider_model_id` en `api-service`, lista cuentas sanas en `credential-manager`, selecciona la mejor cuenta del pool AWS por least-connections + headroom RPM/TPM en Redis, aplica circuit breaker por cuenta con cooldown y reporta outcomes para retroalimentar el algoritmo.
- **Consume:** gRPC de `credential-manager` (`ListHealthyAccountsForModel`), gRPC de `api-service` (`CatalogService.GetModel`), Redis (contadores de carga/circuit breaker en tiempo real).
- **Expone:** gRPC interno (`SelectAccount`, `ReportOutcome`, `ResetCircuitBreaker`, `GetCircuitStatus`, `GetAccountLoad`, `ListAccountLoads`).
- **Publica:** `account.marked_unhealthy`, `account.recovered` a NATS.
- **Operación:** `/health`, `/metrics` y relay `router.event_outbox` → JetStream.
- **No hace:** no ejecuta la llamada HTTP a Bedrock directamente (esa responsabilidad vive en el crate `bedrock-client`, usado por `api-gateway`); `model-router` solo decide *quién* atiende.

### `apps/auth-service`
- **Bounded context:** Identidad de los **usuarios humanos** que usan el gateway (no confundir con las API keys de tráfico de IA, eso es `key-services`).
- **Responsabilidad:** registro/login, hashing de contraseña (Argon2id), emisión y verificación de JWT de sesión + refresh tokens para `customer-bff`/`admin-bff`, MFA (TOTP), verificación de email, flujo de reseteo de contraseña, opcionalmente login social (OAuth2). También gestiona el perfil básico del usuario (`display_name`, `avatar_url`).
- **Tablas propias:** `users` (credenciales + perfil), `refresh_tokens`, `mfa_secrets`, `email_verifications`.

### `apps/user-service`
> **ELIMINADO.** Sin organizaciones ni multi-tenant, el perfil del usuario se absorbe en `auth-service` (`display_name`, `avatar_url` directamente en `auth.users`). No se necesita un servicio separado para datos de perfil.

### `apps/payment-services`
- **Bounded context:** Integración con la pasarela de pago.
- **Responsabilidad:** integración con PayPal. El flujo implementado es `Create Order` → aprobación del comprador → `Capture Order`, con verificación oficial de firma de webhook. También soporta PayPal Vault (`setup-tokens` → `payment-tokens`) para métodos tokenizados, recargas, cobros postpago, deduplicación de webhooks y reconciliación. Wompi, Payoneer y PayU no son proveedores activos del despliegue actual.
- **Tablas propias:** `payment_methods`, `payment_provider_events`, `payment_transactions`.
- **Expone:** gRPC interno autenticado para BFF/admin y HTTP público solo para webhooks del proveedor (`/webhooks/paypal`).
- **Publica:** `payment.succeeded`, `payment.failed` a NATS → consumido por `billing-service`.
- **Robustez:** reintentos con backoff ante errores temporales de PayPal, Outbox transaccional para eventos, audit log de acciones sensibles, webhook endurecido con límite de body, timeout y rate limit, más `/health` y `/metrics` para operación/alertas.

### `apps/key-services`
- **Bounded context:** Ciclo de vida de las **API keys de tráfico** (las que usan tus clientes para llamar al gateway).
- **Responsabilidad:** generación segura (256 bits de entropía, formato `sk-live-<prefix><secret>`), hash + almacenamiento (nunca texto plano), revocación, rotación, scopes/permisos por key, expiración opcional, allowlist de IPs opcional, endpoint gRPC de validación de **muy baja latencia** (cacheado en Redis: `key_hash → {user_id, status, scopes, overrides}` con invalidación activa al revocar). El plan y sus límites se resuelven mediante `subscription-service`, no se duplican en `keys.api_keys`.
- **Tablas propias:** `api_keys`.
- **Nota de diseño:** esta validación es la que más impacto tiene en la latencia p99 del gateway — cachear agresivamente y tener un plan claro de invalidación de cache es crítico.

### `apps/api-service`
- **Bounded context:** Catálogo de modelos y definición de la API pública.
- **Responsabilidad:** fuente de verdad de qué modelos existen, sus capacidades (context window, streaming, tool use, visión), qué planes tienen acceso a cada modelo, versionado del contrato público, generación de la documentación OpenAPI, mapeo entre el nombre "amigable" que ve el cliente y el `model_id` real de Bedrock.
- **Tablas propias:** `model_providers`, `model_catalog`, `plan_model_access`.

### `apps/subscription-service`
- **Bounded context:** Planes y suscripciones.
- **Responsabilidad:** ciclo de vida de planes y suscripciones por usuario: crear/editar/desactivar planes; crear, subir, bajar, cancelar, renovar/reactivar y marcar morosa una suscripción. Expone `PlanResolutionService` para el hot path del gateway y `SubscriptionManagementService` para BFF/admin. Cachea límites vigentes en Redis e invalida esa cache en cada cambio; publica `subscription.changed` por NATS.
- **Tablas propias:** `plans`, `plan_model_access`, `subscriptions`.
- **Expone:** gRPC interno autenticado con JWT de servicio.

### `apps/invoice-service`
- **Bounded context:** Facturación documental.
- **Responsabilidad:** consume `billing.period_closed`, genera factura documental idempotente por `invoice_id`, asigna numeración fiscal correlativa por período, crea line items trazables por modelo desde `usage.usage_events`, genera PDF real y lo almacena en Cloudflare R2, gestiona estados (`draft/open/paid/void/uncollectible`) por gRPC interno autenticado.
- **Tablas propias:** `invoice_sequences`, `invoices`, `invoice_line_items`.
- **Consume:** `billing.period_closed` desde `billing-service`.
- **Expone:** `InvoiceService` para BFF/admin: detalle, listado por usuario, PDF, próximo número y cambios administrativos de estado.
- **Operación:** `/health`, `/metrics` y DLQ `dlq.invoice_service.billing.period.closed`.

### `apps/credential-manager`
- **Bounded context:** Pool de credenciales AWS.
- **Responsabilidad:** alta/baja de cuentas AWS del pool, ejecución de `STS AssumeRole` para obtener credenciales temporales (recomendado sobre Access Keys estáticas), cacheo seguro en memoria de esas credenciales con su TTL, health checks periódicos por cuenta, exposición vía gRPC a `model-router`, cifrado con KMS si en algún caso se necesitan Access Keys estáticas como fallback.
- **Tablas propias:** `aws_credential_pool`, `aws_account_models`.

### `apps/billing-service`
- **Bounded context:** Medición y cobro.
- **Responsabilidad:** consumidor NATS de `payment.succeeded` para recargar wallet, consumidor NATS de `usage.recorded` para reconciliación asíncrona contra reservas, y servicio gRPC síncrono para el patrón de **reserva optimista de saldo** (reservar antes de Bedrock, liberar o confirmar después). Calcula `aws_cost` y `charged_amount` consultando `api-service` (`CatalogService.GetCurrentPricing`); escribe entradas inmutables en `wallet_ledger` y mantiene `wallets` como proyección reconstruible.
- **Tablas propias:** `wallets`, `wallet_ledger`, `transactions`, `reservations`.
- **Consume:** `payment.succeeded`, `usage.recorded`.
- **Expone:** `BillingReservationService` para `api-gateway` y `BillingWalletService` para BFF/admin, ambos por gRPC interno autenticado.
- **Publica:** `wallet.debited`, `wallet.low_balance`, `billing.period_closed`.
- **Operación:** job interno de expiración de reservas vencidas, webhooks salientes firmados, `/health`, `/metrics` y DLQ `dlq.billing_*`.

### `apps/usage-service`
- **Bounded context:** Analítica y reporting.
- **Responsabilidad:** consume `usage.recorded` desde NATS y persiste `usage_events` como proyección analítica idempotente por `request_id`; crea/verifica particiones mensuales; refresca `usage_hourly` y `usage_daily`; expone agregaciones a BFF/admin por gRPC interno autenticado; mantiene `/health`, `/metrics` y DLQ para eventos inválidos.
- **Tablas propias:** `usage_events` (particionada por mes), `usage_hourly`, `usage_daily`.
- **Consume:** `usage.recorded`.
- **Expone:** `UsageReportingService` para uso por usuario, API key, modelo, mes actual, top modelos, tasa de error y percentiles de latencia.
- **Operación:** jobs internos de particiones y refresh de vistas materializadas; DLQ `dlq.usage_service.usage.recorded`.

### `apps/customer-bff`
- **Bounded context:** Backend-for-frontend del dashboard de clientes.
- **Responsabilidad:** agrega llamadas a `auth-service`, `key-services`, `subscription-service`, `invoice-service`, `usage-service`, `payment-services` y `file-service` para armar las pantallas del dashboard (uso, keys, archivos, RAG, facturación, plan actual y perfil). Gestiona la sesión del navegador con cookies `httpOnly` + `Secure` + `SameSite=Strict`, usando el JWT emitido por `auth-service`. El frontend de clientes usa este BFF para control y metadatos; las cargas grandes van directamente a R2 mediante URLs firmadas.

### `apps/admin-bff`
- **Bounded context:** Backend-for-frontend del panel administrativo interno.
- **Responsabilidad:** igual patrón que `customer-bff` pero para operadores internos — ver todos los usuarios, gestionar el pool de cuentas AWS (vía `credential-manager`), ver salud del sistema y circuit breakers activos, ajustar pricing (`model_pricing`), resolver disputas de facturación, forzar el estado de una cuenta AWS. Requiere rol `admin` verificado por `auth-service` y debe estar en una red/dominio separado del dashboard de clientes (nunca compartir el mismo subdominio ni cookies).

### `apps/file-service`
- **Bounded context:** Archivos, cuotas e ingestión RAG.
- **Responsabilidad:** dueño de `files`, `folders`, cuotas, reservas de ingestión,
  estados, auditoría, referencias de chunks y vectores canónicos. Genera URLs
  firmadas de R2, confirma hashes/MIME/tamaño, publica Outbox a JetStream,
  extrae PDF/DOCX/XLSX/TXT, genera embeddings vía OpenRouter y consulta Qdrant
  con filtro obligatorio por `user_id`.
- **Persistencia externa:** base `files_db`, Cloudflare R2 y Qdrant. Los
  originales se guardan en R2; PostgreSQL conserva metadatos y referencias.
- **Expone:** `FileService` gRPC autenticado, incluido uso de almacenamiento,
  búsqueda vectorial, reindexado y borrado coordinado.
- **Asíncrono:** consumidores durables para ingestión y borrado vectorial con
  ACK, reintentos, backpressure y DLQ.

### `apps/s3-gateway`
- **Bounded context:** Compatibilidad S3 para Pro Plus.
- **Responsabilidad:** API S3-compatible sobre Cloudflare R2, sin usar AWS S3.
  Valida SigV4, scopes de API key y delega ownership/cuotas al `file-service`.
- **Capacidades:** operaciones de objetos, listado, streaming, `Range`/206,
  multipart upload y hash en streaming. Las claves se fuerzan al namespace
  privado del `user_id` autenticado.
- **Exposición:** HTTP en `storage.jnmtortas.com`/puerto configurado; no es un
  BFF y no acepta `user_id`, colección Qdrant ni claves de objeto arbitrarias
  desde el cliente.

## 5. Crates compartidos (`crates/`)

| Crate | Contenido |
|---|---|
| `proto` | Definiciones `.proto` y código generado por `tonic-build`. Es el contrato formal entre servicios — cualquier cambio de campo es un cambio versionado, nunca "silencioso". |
| `db` | Pool de conexión Postgres (`sqlx` + `deadpool-postgres` o `sea-orm`), helpers de migración, tipos comunes (`Money`, `Pagination`). |
| `events` | Structs `serde` compartidos que representan cada evento de NATS (`UsageRecorded`, `PaymentSucceeded`, etc.) — garantiza que publisher y consumer nunca se desincronicen de esquema. |
| `bedrock-client` | Wrapper sobre `aws-sdk-bedrockruntime`. Maneja `Converse`/`ConverseStream`, extrae `usage.input_tokens`/`usage.output_tokens` de la respuesta, traduce errores de Bedrock a errores de dominio. |
| `common` | Tipos de error compartidos (`thiserror`), carga de configuración (`config`/`envy`), setup de `tracing` + OpenTelemetry, middlewares comunes (extractor de `request_id`, extractor de API key). |

La observabilidad se despliega como infraestructura independiente: OpenTelemetry
Collector exporta trazas a Tempo; Alloy envía logs a Loki; Prometheus recoge
métricas y Grafana consulta las tres fuentes. Vault administra los secretos y
material TLS/mTLS; Postgres, Redis, NATS JetStream y Qdrant se ejecutan como
infraestructura local, no como microservicios Rust.

## 6. Flujo de facturación paso a paso

1. `api-gateway` recibe la petición y, **antes** de llamar a Bedrock, pide a `billing-service` (vía gRPC síncrono, no NATS) una **reserva** de saldo basada en una estimación conservadora de tokens (o en `max_tokens` del request).
2. Si el saldo (balance − reservado) no cubre la reserva → `402 Payment Required` inmediato, sin llamar a AWS.
3. Se ejecuta la llamada real a Bedrock.
4. Al recibir la respuesta con el conteo real de tokens, `api-gateway` publica `usage.recorded` a NATS (asíncrono, no bloquea la respuesta al cliente).
5. `billing-service` consume el evento, calcula el costo real con `model_pricing` vigente, libera la reserva y aplica el débito real (que puede ser menor o mayor a lo reservado), escribe la entrada en `wallet_ledger`.
6. `usage-service` consume el mismo evento y persiste su proyección analítica idempotente; sus jobs refrescan `usage_hourly`/`usage_daily` para dashboards.
7. Si el modelo es postpago, al cierre del periodo `billing-service` publica `billing.period_closed` → `invoice-service` genera la factura agregando los `usage_events` del ciclo.

> El paso 1 (reserva síncrona) es la diferencia clave frente al diseño original: sin reserva, dos peticiones concurrentes de un usuario con saldo justo pueden ambas pasar la validación de saldo "actual" y dejarte con balance negativo. Con reserva, el saldo disponible baja inmediatamente al iniciar cada petición.

## 7. Notas de escalabilidad

- **Particionamiento de `usage_events` por mes** (`RANGE` sobre `created_at`) — permite `DROP`/archivar particiones viejas sin `DELETE` masivo y mantiene los índices pequeños.
- **Cache L1 en el propio `api-gateway`** (in-memory, TTL corto) + **L2 en Redis** para la validación de API key, para no depender de una llamada gRPC en el hot path de cada request.
- **Read replicas de Postgres** para los BFF (lecturas de dashboard) separadas del primary que recibe las escrituras de `billing-service`/`usage-service`.
- **`api-gateway` y `model-router` son stateless** → escalan horizontalmente detrás de un load balancer sin coordinación adicional.
- **NATS JetStream con `ack` explícito y DLQ (dead-letter)** para los eventos de billing — un evento de uso perdido es dinero perdido, nunca debe descartarse silenciosamente.
