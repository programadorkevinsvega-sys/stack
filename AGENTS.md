# Repository Guidelines

## Estado y alcance

Este repositorio es un workspace Rust 2021 con 20 paquetes: 15 aplicaciones
desplegables y 5 crates compartidos. Las aplicaciones actuales son:

`api-gateway`, `customer-bff`, `admin-bff`, `api-service`, `auth-service`,
`key-services`, `subscription-service`, `payment-services` (PayPal),
`credential-manager`, `model-router`, `billing-service`, `usage-service`,
`invoice-service`, `file-service` y `s3-gateway`.

Los BFF y gateways exponen HTTP; la comunicación interna de negocio es gRPC y
los procesos asíncronos usan el envelope tipado de `crates/events` sobre NATS
JetStream. Cada servicio usa una base PostgreSQL independiente dentro del
contenedor local compartido; no se usan esquemas compartidos.

## Estructura

- `apps/`: servicios desplegables; la estructura física puede variar por
  bounded context, pero debe conservar la separación entre dominio, aplicación,
  infraestructura e interfaces cuando el servicio la necesite.
- `crates/`: `common`, `db`, `events`, `proto` y `bedrock-client`.
- `client-platform-dashboard/` y `admin-plataform-dashboard/`: frontends
  Next.js.
- `docker-compose.yml`: solamente infraestructura local (PostgreSQL, Redis,
  NATS, Qdrant, Vault y observabilidad). Los microservicios Rust y frontends
  se compilan y ejecutan fuera de Docker.
- `vault/`, `scripts/`, `artifacts/verify/`: secretos/bootstrap, operación y
  evidencias de verificación.

## Comandos de build, pruebas y ejecución

- `./build-all.sh`: compila los binarios Rust en modo release y ambos
  frontends; es el único paso de compilación de producción.
- `./run-all.sh`: verifica dependencias, ejecuta migraciones y arranca los
  binarios release existentes; no debe compilar, iniciar frontends ni crear
  contenedores.
- `./run-all.sh --stop`: detiene procesos backend iniciados por el script.
- `docker compose up -d`: levanta la infraestructura local.
- `cargo check --workspace`: validación rápida.
- `cargo test --workspace`: pruebas unitarias e integración disponibles.
- `cargo fmt --all` y `cargo clippy --workspace --all-targets --all-features
  -- -D warnings`: formato y lint estricto.
- `sqlx migrate run --source apps/<service>/migrations`: migración puntual,
  usando la base del servicio correspondiente.
- `scripts/verify_fase*.sh`: gates que generan JSON, puntero `latest.json` y
  log en `artifacts/verify/<fase>/`.

## Disciplina de código

- Rust 2021, `rustfmt`, `snake_case` para módulos/funciones, `PascalCase` para
  tipos y `SCREAMING_SNAKE_CASE` para constantes.
- Todo log pasa por `tracing`; están prohibidos `println!`, `dbg!` y salidas
  directas desde librerías.
- No se dejan `TODO`, `todo!()`, `unimplemented!()`, `panic!()`,
  `unreachable!()`, `allow(...)`, imports sin uso ni variables descartadas
  con `_` para ocultar errores. Cada función entregada debe estar completa y
  manejar explícitamente sus errores.
- No se inventan servicios, contratos ni campos. Los cambios de Protobuf o
  eventos incrementan versión de forma explícita.
- La identidad `user_id` se deriva del contexto autenticado y es obligatoria
  en repositorios, R2 y Qdrant; nunca se acepta del cliente como autoridad.
- No se exponen secretos ni PII innecesaria en logs. Usa Vault para claves,
  contraseñas, tokens y material TLS; `.env.example` documenta configuración no
  sensible.

## Arquitectura y seguridad

- HTTP público: `api-gateway`, `customer-bff`, `admin-bff`, `s3-gateway`.
- gRPC interno protegido por mTLS; Postgres, Redis, NATS, Qdrant y Vault son
  dependencias de infraestructura local y no se modifican para añadir TLS
  salvo una decisión de despliegue explícita.
- Eventos: Outbox PostgreSQL → PubAck JetStream → consumidores idempotentes
  con ACK, reintentos y DLQ.
- Archivos: R2 para objetos, Qdrant para vectores, `file-service` como
  autoridad de cuotas/autorización y `s3-gateway` como compatibilidad S3
  mediante SigV4.
- Observabilidad: OpenTelemetry/Tempo, Prometheus/Grafana y Loki/Alloy.

## Testing

- Pruebas puras para invariantes de dominio y pruebas con PostgreSQL/Redis/NATS
  reales para persistencia, aislamiento e idempotencia. No se usan mocks como
  sustituto del comportamiento real.
- No usar `sleep()` para esperar eventos: usar condiciones, relojes/control
  determinista o dependencias reales con espera activa.
- Toda integración verifica aislamiento entre usuarios, redelivery de eventos,
  expiración de leases, errores 429/5xx, DLQ y recuperación.
- El fallo DNS histórico de `common::ssrf::tests::allows_public_https` se
  documenta separadamente y no se atribuye a cambios de TLS.

## Cambios y revisión

Usar commits Conventional Commits cuando exista historial Git. Toda PR debe
describir objetivo, servicios/crates afectados, migraciones/env nuevos y la
evidencia de `cargo check`, tests, lint y gates de fase.
