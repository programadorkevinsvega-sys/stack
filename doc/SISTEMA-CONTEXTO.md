# Contexto del Sistema — Nexus AI Gateway

> Reconciliado con el workspace y la infraestructura actual el 2026-07-25.

## Stack Tecnológico

| Componente | Tecnología |
|------------|-----------|
| **Backend (microservicios)** | Rust 2021 edition, workspace Cargo |
| **HTTP público** | Axum 0.8 en `api-gateway`, `customer-bff`, `admin-bff` y `s3-gateway` |
| **gRPC interno** | Tonic 0.14 + Protobuf |
| **Base de datos** | PostgreSQL 16 vía SQLx 0.8; bases independientes por servicio en un contenedor compartido |
| **Cache / sesiones** | Redis 7 vía deadpool-redis |
| **Mensajería durable** | NATS 2.10 JetStream vía async-nats (Outbox, PubAck, replay y DLQ) |
| **Mensajería realtime / control plane** | NATS 2.10 |
| **Frontend** | Next.js 16, React 19, shadcn/ui, Tailwind CSS v4 |
| **Testing (Rust)** | cargo test, pruebas reales PostgreSQL/Redis, proptest; testcontainers solo donde esté configurado |
| **Embeddings** | OpenRouter API OpenAI-compatible, `qwen/qwen3-embedding-8b` |
| **Archivos/vectorial** | Cloudflare R2 + Qdrant, gestionados por `file-service` y `s3-gateway` |
| **Observabilidad** | OTEL Collector + Tempo, Alloy + Loki, Prometheus + Grafana |
| **Secretos** | Vault KV/PKI/AppRole; `.env` conserva configuración no sensible |
| **Testing (frontend)** | Vitest v4, React Testing Library, jsdom |

---

## Estructura del Proyecto

```
api-gateway-ai-aws/
├── apps/                          # Microservicios deployables
│   ├── api-gateway/               # Gateway edge (Axum + gRPC)
│   ├── api-service/               # Catálogo de modelos (gRPC)
│   ├── admin-bff/                 # BFF de administración (Axum HTTP)
│   ├── auth-service/              # Autenticación y MFA (gRPC)
│   ├── billing-service/           # Facturación y reservas (HTTP + gRPC)
│   ├── credential-manager/        # Proveedores AWS (gRPC)
│   ├── customer-bff/              # BFF para frontend (Axum HTTP)
│   ├── file-service/              # Metadatos, cuotas, R2, ingestión y Qdrant
│   ├── invoice-service/           # Facturas PDF (HTTP + gRPC)
│   ├── key-services/              # API keys (gRPC)
│   ├── model-router/              # Ruteo a Bedrock Mantle (HTTP + gRPC)
│   ├── payment-services/          # Pasarela PayPal (HTTP + gRPC)
│   ├── subscription-service/      # Planes y suscripciones (gRPC)
│   ├── usage-service/             # Métricas de uso (HTTP + gRPC)
│   └── s3-gateway/                # API S3-compatible sobre Cloudflare R2
├── crates/                        # Librerías compartidas
│   ├── common/                    # Config, errores, TLS, JWT, SSRF, auditoría
│   ├── db/                        # Pool de conexiones (PG, Redis, NATS health)
│   ├── events/                    # Eventos canónicos + payloads tipados
│   ├── proto/                     # Contratos y stubs gRPC versionados
│   └── bedrock-client/            # Cliente HTTP para chat Bedrock/Mantle
├── client-platform-dashboard/     # Frontend Next.js
├── admin-plataform-dashboard/     # Frontend Next.js de administración
├── docker-compose.yml             # Infra: Postgres, Redis, NATS, Qdrant, Vault y observabilidad
├── vault/                         # Configuración, política y bootstrap automático de Vault PKI
└── scripts/                       # Scripts auxiliares
```

### Apps con migraciones SQL

Las migraciones de cada app se ejecutan contra su propia base de datos. No se
usan esquemas con nombre ni `search_path` compartidos.

| App | Migraciones | Propósito |
|-----|-------------|-----------|
| credential-manager | 2 | `init_credentials`, `aws_account_unique` |
| auth-service | 2 | `init_auth`, `add_audit_signature` |
| key-services | 1 | `init_keys` |
| api-service | 1 | `init_catalog` |
| subscription-service | 2 | `init_subscription`, `add_active_subscription_unique` |
| payment-services | 2 | `init_payments`, `add_provider_event_unique` |
| billing-service | 1 | `init_billing` |
| usage-service | 1 | `init_usage` |
| invoice-service | 1 | `init_invoicing` |
| file-service | 13 | metadatos, carpetas, cuotas, ingesta, vectores y auditoría |

### Crates compartidos

| Crate | Responsabilidad |
|-------|----------------|
| **common** | `AppConfig`, `DomainError`, `Money`, `Clock`, `ServiceJwtAuth`, `AuditSigner`, `PgAuditLogger`, `GrpcRateLimiter`, `KeysetPagination`, `SsrfError`, `ServiceTlsConfig`, `RequestId`, `shutdown` |
| **db** | `PgPool`, `RedisPool`, `DbConfig`, `check_health()` |
| **events** | `EventEnvelope<T>` con 14 tipos, incluidos autorización de API keys, ingesta y borrado de vectores |
| **proto** | Contratos/stubs gRPC versionados para los servicios internos actuales |
| **bedrock-client** | Cliente HTTP para chat Bedrock/Mantle: `converse()`, `converse_stream()`; embeddings se resuelven por OpenRouter |

---

## Frontend Dashboard

### Stack
- **Framework**: Next.js 16.2.6 (App Router)
- **UI**: shadcn/ui (basado en @base-ui/react + Tailwind v4)
- **Formularios**: react-hook-form + @hookform/resolvers + Zod v4
- **Estado**: Zustand (sesión), TanStack Query (server state)
- **Charts**: Recharts
- **Notificaciones**: Sonner (toast)

### Estructura de páginas

```
app/
├── (auth)/
│   ├── login/page.tsx          # Login con MFA opcional
│   ├── register/page.tsx       # Registro con validación
│   └── verify-email/page.tsx   # Verificación de correo
├── (dashboard)/
│   ├── layout.tsx              # Sidebar + user menu + ErrorBoundary
│   ├── dashboard/page.tsx      # Resumen de uso (charts)
│   ├── api-keys/page.tsx       # CRUD API keys
│   ├── models/page.tsx         # Catálogo de modelos
│   ├── subscription/page.tsx   # Plan activo + cancelación
│   ├── invoices/page.tsx       # Historial de facturas
│   ├── wallet/page.tsx         # Saldo + recargas
│   ├── payment-methods/page.tsx # Métodos de pago
│   ├── profile/page.tsx        # Perfil + MFA + cambio password
│   └── files/page.tsx          # Archivos, carpetas, cuota y estado RAG
├── plans/page.tsx              # Planes disponibles
├── subscription/confirm/page.tsx # Confirmación post-pago
├── layout.tsx                  # Root layout (Providers + ErrorBoundary)
├── robots.ts                   # SEO (generado estáticamente)
└── sitemap.ts                  # SEO (generado estáticamente)
```

### Capa de API (`lib/api/`)

| Archivo | Endpoints |
|---------|-----------|
| `client.ts` | Core fetch wrapper: CSRF, refresh token slide-session, 401 handler, retry 403 |
| `auth.ts` | login, register, verifyEmail, me, updateProfile, changePassword, enableMfa, confirmMfa, disableMfa, logout, logoutAll |
| `apiKeys.ts` | list, create, revoke |
| `usage.ts` | summary, topModels, latency |
| `plans.ts` | list |
| `subscription.ts` | get, subscribe, cancel |
| `wallet.ts` | get, topUp |
| `invoices.ts` | list, get |
| `paymentMethods.ts` | list, setDefault |

### Tests

| Archivo | Tests |
|---------|-------|
| `lib/__tests__/validation.test.ts` | ~20 tests sobre Zod schemas (login, register, apiKey, topUp, profile, changePassword) |
| `lib/__tests__/client.test.ts` | ~17 tests sobre client.ts (GET, POST, PUT, PATCH, DELETE, CSRF, refresh token, 401, 403, 422) |
| `components/__tests__/login-form.test.tsx` | 3 tests (render, register link, submit) |
| `components/__tests__/payment-provider-selector.test.tsx` | 6 tests (render all, onChange, selected, radiogroup, ProviderMark) |
| `components/__tests__/brand.test.tsx` | 2 tests (BrandMark, BrandLogo) |
| **Total** | **~50 tests** |

### Configuración de entorno (dashboard)
```env
NEXT_PUBLIC_BFF_URL=http://localhost:4000   # URL del customer-bff
```

---

## Estado de implementación

El código principal de planes, archivos, RAG, autorización, administración y
observabilidad está implementado. La interoperabilidad S3, carga/restore,
recuperación completa de Qdrant y las pruebas de carga se cierran solamente
con ejecución en staging y evidencias en `artifacts/verify/`.

## Lo que ya está implementado

### Fase 1 — Core y Configuración Inicial
- [x] Providers (ThemeProvider, QueryClient, ErrorBoundary, Toaster)
- [x] Session store (Zustand con initialized flag)
- [x] Fetch wrapper (CSRF doble submit, refresh token slide-session, 401 handler)
- [x] Validación Zod (loginSchema, registerSchema, createApiKeySchema, topUpSchema, profileSchema, changePasswordSchema)
- [x] Tipos compartidos (User, ApiKey, Invoice, Subscription, Wallet, Model, Plan, etc.)

### Fase 2 — Componentes Base
- [x] shadcn/ui base (Button, Input, Label, Card, Skeleton, Badge, Dialog, DropdownMenu, Tabs, Table)
- [x] Componentes de negocio (Brand, ThemeToggle, PaymentProviderSelector, PlansGrid, NavItems)
- [x] ErrorBoundary global (con role="alert", auto-focus heading, Reintentar/Inicio)

### Fase 3 — Páginas del Dashboard (implementadas durante Fase 2)
- [x] Dashboard/Home (charts de uso, top models, latency)
- [x] API Keys (list, create, revoke con confirmación)
- [x] Models (catálogo con pricing)
- [x] Subscription (plan actual, cancelar con confirmación, upgrade)
- [x] Invoices (historial con paginación)
- [x] Wallet (saldo, top-up)
- [x] Payment Methods (listar, default)
- [x] Profile (editar perfil, MFA, cambio de password)

### Fase 4 — Sesión y Refresh Token
- [x] Envío automático de `x-refresh-token` en cada request
- [x] Lectura de nuevo refresh token desde header `x-refresh-token`
- [x] `credentials: "include"` para Set-Cookie
- [x] Middleware BFF con validación `/api/v1/me` y forward de Set-Cookie

### Fase 5 — UX y Errores
- [x] ErrorBoundary global (root + dashboard layout)
- [x] Errores 422 por campo en login (setError react-hook-form)
- [x] Skeletons consistentes en todas las páginas
- [x] Empty states en todas las páginas
- [x] Confirmaciones destructivas (revoke key, cancelar suscripción)

### Fase 6 — Testing
- [x] Setup Vitest + RTL + jsdom
- [x] Tests de validación (Zod schemas)
- [x] Tests de API layer (client.ts)
- [x] Tests de componentes (LoginForm, PaymentProviderSelector, Brand)
- [x] Scripts: `bun run test` / `bun run test:watch`

### Fase 7 — Hardening y Producción
- [x] `.env.example` con todas las variables documentadas
- [x] CI/CD (GitHub Actions: lint → typecheck → test → build)
- [x] Dockerfile multi-stage (standalone output)
- [x] CSP Headers + security headers (X-Frame-Options, X-Content-Type-Options, Referrer-Policy)
- [x] TypeScript strict (`ignoreBuildErrors` eliminado, 0 errores)
- [x] SEO (sitemap.xml, robots.txt, OpenGraph, Twitter cards)
- [x] Bundle analyzer (`ANALYZE=true` toggle)
- [x] Accesibilidad (aria-* en formularios, sidebar, error boundary, focus management)

### Eliminación de Mocks
- [x] MSW eliminado (`lib/mocks/` borrado)
- [x] Mock-checkout page eliminada
- [x] Mock mode de config.ts eliminado (BFF_URL now required)
- [x] Credenciales demo del login eliminadas
- [x] Dependencia msw eliminada de package.json
- [x] Body fallback de refresh token eliminado (solo header)

---

## Arquitectura de Comunicación

```
                    ┌─────────────────┐
                    │   Next.js App   │
                    │ (Dashboard)     │
                    └────────┬────────┘
                             │ HTTPS
                             ▼
                    ┌─────────────────┐
                    │  customer-bff   │  ← Axum HTTP server (BFF)
                    │  (Axum)         │
                    └────────┬────────┘
                             │ gRPC (tonic) + NATS
           ┌─────────────────┼────────────────────┐
           ▼                 ▼                    ▼
    ┌────────────┐   ┌──────────────┐   ┌────────────────┐
    │ auth-svc   │   │ key-services  │   │ subscription   │
    │ (gRPC)     │   │ (gRPC)       │   │ (gRPC)         │
    └─────┬──────┘   └──────┬───────┘   └───────┬────────┘
          │                 │                    │
    ┌─────▼──────┐   ┌──────▼───────┐   ┌───────▼────────┐
    │ api-svc    │   │ credential   │   │ payment-svc    │
    │ (catalog)  │   │ manager      │   │ (gRPC+HTTP)    │
    └─────┬──────┘   └──────┬───────┘   └───────┬────────┘
          │                 │                    │
    ┌─────▼──────┐   ┌──────▼───────┐   ┌───────▼────────┐
    │ model      │   │ billing      │   │ invoice-svc    │
    │ router     │   │ service      │   │ (gRPC+S3)      │
    └─────┬──────┘   └──────┬───────┘   └───────┬────────┘
          │                 │                    │
          └─────────────────┼────────────────────┘
                            │ NATS
                    ┌───────▼────────┐
                    │  usage-svc    │
                    │ (gRPC+HTTP)   │
                    └───────────────┘
```

### Contratos de eventos (crates/events)

Los eventos viajan por NATS JetStream. El formato canónico es `EventEnvelope<T>`:

| Campo | Tipo |
|-------|------|
| event_id | Uuid |
| event_type | String (constante del crate events) |
| schema_version | u32 |
| occurred_at | DateTime<Utc> |
| trace_id | Uuid |
| span_id | Option<Uuid> |
| request_id | Option<String> |
| payload | T (el tipo de evento específico) |

---

## Lo que falta / Pendiente

### Backend (Rust)

| Item | Prioridad | Detalle |
|------|-----------|---------|
| **gRPC health checks** | Media | Todos los servicios exponen gRPC pero no todos implementan el proto `grpc.health.v1.Health`. |
| **TLS/mTLS entre servicios** | Implementado | `crates/common/src/tls.rs` protege exclusivamente gRPC entre microservicios con selección `TLS_CERT_PROVIDER=local|aws|azure|vault`; PostgreSQL, Redis, NATS/JetStream y Vault no se alteran. Permanece deshabilitado por defecto para desarrollo. |
| **Vault PKI + secretos local** | Implementado | `docker-compose.yml` inicia Vault persistente y `vault-bootstrap` automatiza init, unseal, CA, rol PKI, KV v2, política y AppRole. Contraseñas, tokens, peppers y claves se retiran de `.env`, se guardan en Vault y se proyectan con permisos `0600`. |
| **Rate limiting global** | Media | `crates/common/src/grpc_rate_limit.rs` existe pero no está integrado en todos los servicios. |
| **Integration tests** | Media | Hay pruebas reales de PostgreSQL/Redis y gates adversariales; ampliar la matriz de todos los servicios en staging. `testcontainers` es opcional, no requisito de producción. |
| **Read models / projections** | Media | Para CQRS: crear tablas de lectura/read models en PostgreSQL que los query services (customer-bff) consulten. |
| **Admin BFF** | Baja | Implementado en `apps/admin-bff`; falta únicamente validar flujos administrativos completos en staging. |
| **CI/CD para Rust** | Alta | No hay GitHub Actions para el workspace Rust. Solo existe CI para el dashboard. |
| **Dockerfile Rust** | Media | No aplica por decisión del proyecto: los microservicios Rust se compilan y ejecutan manualmente; Docker se reserva para infraestructura. |
| **sbom / supply chain** | Baja | No hay generación de SBOM ni verificación de dependencias. |

### Frontend (Dashboard)

| Item | Prioridad | Detalle |
|------|-----------|---------|
| **Archivos/RAG** | Media | `/files`, cuotas y estado de ingestión están implementados; validar UX de cargas grandes y errores de R2 en staging. |
| **Admin Dashboard** | Media | Está implementado; validar métricas, DLQ, bloqueo y acciones auditadas con datos reales. |

### Infraestructura

| Item | Prioridad | Detalle |
|------|-----------|---------|
| **Secrets en archivo JSON** | **Alta** | Vault es la fuente de verdad; revisar cualquier archivo heredado antes de producción y mantenerlo fuera de Git. |
| **Monitoring stack** | Media | Implementado con Prometheus/Grafana, OTEL/Tempo y Loki/Alloy; faltan únicamente validaciones de alertas en staging. |
| **Load testing** | Baja | Scripts Fase 9 existen; su ejecución contra staging sigue pendiente. |

---

## Contratos importantes

### Endpoints del BFF (customer-bff)

El BFF expone endpoints REST que el frontend consume:

| Método | Path | Descripción |
|--------|------|-------------|
| POST | `/api/v1/auth/register` | Registro |
| POST | `/api/v1/auth/verify-email` | Verificar correo |
| POST | `/api/v1/auth/login` | Login (retorna user + refresh_token en header/body) |
| GET | `/api/v1/me` | Perfil del usuario autenticado |
| PATCH | `/api/v1/me` | Actualizar perfil |
| POST | `/api/v1/auth/logout` | Cerrar sesión |
| POST | `/api/v1/auth/change-password` | Cambiar contraseña |
| POST | `/api/v1/auth/enable-mfa` | Habilitar MFA |
| POST | `/api/v1/auth/confirm-mfa` | Confirmar MFA |
| POST | `/api/v1/auth/disable-mfa` | Deshabilitar MFA |
| GET | `/api/v1/api-keys` | Listar API keys |
| POST | `/api/v1/api-keys` | Crear API key |
| DELETE | `/api/v1/api-keys/:id` | Revocar API key |
| GET | `/api/v1/usage` | Resumen de uso |
| GET | `/api/v1/usage/top-models` | Modelos más usados |
| GET | `/api/v1/usage/latency` | Estadísticas de latencia |
| GET | `/api/v1/plans` | Listar planes |
| GET | `/api/v1/subscription` | Suscripción actual |
| POST | `/api/v1/subscription` | Crear suscripción |
| POST | `/api/v1/subscription/cancel` | Cancelar suscripción |
| GET | `/api/v1/wallet` | Saldo de wallet |
| POST | `/api/v1/top-up` | Recargar wallet |
| GET | `/api/v1/payment-methods` | Métodos de pago |
| PUT | `/api/v1/payment-methods/:id/default` | Default payment method |
| GET | `/api/v1/invoices` | Facturas |
| GET | `/api/v1/invoices/:id` | Detalle de factura |
| GET | `/api/v1/models` | Catálogo de modelos |

### Refresh Token flow

1. El BFF setea `x-refresh-token` header en la respuesta de login
2. El frontend (`client.ts`) guarda el token en memoria (nunca localStorage)
3. Cada request envía `x-refresh-token` en el header para slide-session
4. Cada respuesta actualiza el token desde el header
5. Si el BFF no envía header, no hay cambio de token
6. En 401, se limpia la sesión y redirige a /login

### CSRF Protection

1. El BFF setea cookie `csrf_token` (accesible desde JS) en login
2. Toda mutación (POST/PUT/PATCH/DELETE) envía `X-CSRF-Token` desde la cookie
3. Si el BFF responde 403, el frontend reintenta una vez con cookie fresca

---

## Comandos de desarrollo

```bash
# Infraestructura local
docker compose up -d

# Build workspace Rust
cargo check --workspace
cargo build --release -p auth-service  # o cualquier app

# Correr un servicio
cargo run -p auth-service

# Tests Rust
cargo test --workspace

# Dashboard
cd client-platform-dashboard
bun install
NEXT_PUBLIC_BFF_URL=http://localhost:4000 bun run dev

# Tests dashboard
bun run test

# Lint global
cargo clippy --workspace --all-targets --all-features -D warnings

# Scripts de build/run
./build-all.sh        # Build todo (Rust + dashboard)
./run-all.sh          # Migrar y levantar solo los backends Rust en el host
```

`run-all.sh` no ejecuta contenedores ni frontend. La infraestructura se puede
levantar con `docker compose up -d`; los binarios Rust y los dos frontends se
ejecutan en el host.

`cargo check --workspace`, los tests, `build-all.sh` y `run-all.sh --build`
usan los metadatos versionados de `.sqlx`, por lo que no requieren PostgreSQL
durante la compilación. Después de cambiar una consulta `query!` o una
migración, se levanta la infraestructura y se regeneran y validan contra las
bases reales con `scripts/prepare-sqlx-offline.sh`.

---

## Variables de entorno requeridas

Ver `.env.example` en la raíz para la documentación completa. Las más importantes:

```
# Database (credenciales y URLs reales se proyectan desde Vault)
LOCAL_DATABASES_ENABLED=true
POSTGRES_DATABASES=gateway_db,router_db,credentials_db,auth_db,keys_db,catalog_db,subscription_db,payments_db,billing_db,usage_db,invoicing_db,customer_bff_db,files_db

# Interfaces locales
PGADMIN_DEFAULT_EMAIL=admin@nexusgateway.com
QDRANT_URL=http://localhost:6333

# Redis/NATS
NATS_USERNAME=nats_admin

# BFF (dashboard)
NEXT_PUBLIC_BFF_URL=http://localhost:4000

# Observabilidad
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
OBSERVABILITY_USER_HASH_SALT= # se proyecta desde Vault; no se publica en frontend
```

---

## Notas de seguridad

- **Zero Trust**: Todos los servicios se autentican via ServiceJWT (`crates/common/src/service_jwt.rs`). No hay信任 de red.
- **TLS/mTLS**: `crates/common/src/tls.rs` aplica TLS o mTLS exclusivamente a gRPC entre microservicios. Los certificados pueden proceder de archivos locales, AWS Secrets Manager, Azure Key Vault o Vault KV/PKI. PostgreSQL, Redis, NATS/JetStream y Vault mantienen sus conexiones configuradas de forma independiente. Deshabilitado por defecto en desarrollo; contrato completo en `TLS-MTLS-MULTICLOUD.md`.
- **CSRF**: Implementado con doble submit cookie pattern.
- **Refresh token**: Solo en memoria RAM, nunca persistido.
- **API keys**: Almacenadas con hash HMAC-SHA256 con pepper (`API_KEY_PEPPER`).
- **Passwords**: Hasheadas con argon2.
- **Auditoría**: Cada mutación importante se registra con firma HMAC (`crates/common/src/audit_signer.rs`).
- **SSRF**: `crates/common/src/ssrf.rs` previene Server-Side Request Forgery en el model-router.
