# Roadmap a Producción — Nexus AI Gateway

> Reconciliado con el workspace actual el 2026-07-25. Los estados marcados
> como pendientes son validaciones operativas reales, no funcionalidades que
> falten en el código.

La entrada operativa del proyecto es [`README.md`](../README.md). Este documento
es la fuente de verdad del roadmap; `00`, `01`, `EVENTING`, `VAULT`,
`SISTEMA-CONTEXTO` y `PLAN-IMPLEMENTACION-PLANES-STORAGE-RAG` detallan sus
respectivos contratos.

## Stack real

| Componente | Tecnología | Estado |
|------------|-----------|--------|
| **Backend** | Rust 2021, 15 apps backend/BFF (`cargo metadata`: 20 paquetes incluyendo 5 crates) | Implementado ✅ |
| **Base de datos** | PostgreSQL 16 vía SQLx | Bases independientes por servicio en un contenedor compartido; migraciones automáticas ✅ |
| **Cache / sesiones** | Redis 7 vía deadpool-redis | Infra lista |
| **Backbone de eventos** | **NATS JetStream** (con ACK explícito y DLQ) | Outbox relay, consumidores pull, ACK y DLQ implementados ✅ |
| **Frontend** | Next.js 16, React 19, shadcn/ui | Dashboard completo, conectado a BFF |
| **BFF** | customer-bff + admin-bff (Axum, gRPC clients) | Implementados; builds y tests validados |
| **Archivos/RAG** | file-service + s3-gateway + R2 + Qdrant + OpenRouter/Qwen3 | Implementado; interoperabilidad S3 y validación operativa pendientes |
| **Observabilidad** | OTEL Collector, Tempo, Loki, Alloy, Prometheus y Grafana | Infraestructura y dashboards implementados |
| **Secretos/TLS** | Vault KV/PKI y mTLS gRPC | Automatizado; dependencias de datos mantienen su transporte configurado |

> **Importante**: El backbone de eventos es **NATS JetStream**, no Kafka. NATS JetStream provee persistencia en disco, replay de eventos, ACK explícito, DLQ, y todas las garantías que necesita un sistema de producción. No hay Kafka en el diseño ni en la arquitectura.

---

## Fase 0 — Seguridad y secretos

| # | Item | Archivo | Acción |
|---|------|---------|--------|
| 0.1 | **Credenciales en texto plano** | `.env`, JSON y archivos de runtime | ✅ Migradas a Vault; `.runtime/vault` está excluido y los secretos se proyectan con permisos `0600`. |
| 0.2 | **Secrets en git** | `.gitignore` | ✅ Verificado para los archivos locales protegidos. |
| 0.3 | **Validar que no hay secrets hardcodeados** | Todo el repo | ✅ Las credenciales operativas se resuelven desde Vault; las variables no sensibles permanecen en `.env.example`. |

---

## Fase 1 — Event Relay (Outbox → NATS JetStream)

**Estado actual**: Los productores implementados persisten el envelope en
Outbox y el relay publica a JetStream después del PubAck. Los consumidores son
at-least-once e idempotentes; la DLQ y el reproceso manual quedan operativos.

| # | Item | Detalle | Archivos involucrados |
|---|------|---------|----------------------|
| 1.1 | **Tabla outbox en cada servicio** | ✅ Tablas por bounded context; operaciones SQL críticas escriben el evento en la misma transacción. | `apps/*/migrations/*event_outbox.sql` |
| 1.2 | **Outbox relay** | ✅ Relay compartido con claim concurrente, lease, backoff, PubAck y deduplicación. | `crates/events/src/outbox.rs` |
| 1.3 | **Modelo de suscripción JetStream** | ✅ Pull consumers durables con filtro, ACK explícito confirmado y máximo de entregas. | `apps/*/infrastructure/messaging.rs` |
| 1.4 | **DLQ + alerta** | ✅ Stream `EVENTS_DLQ`, DLQ tras agotar reintentos y métrica `events_dlq_total`. Ver runbook de reproceso manual. | `EVENTING-OUTBOX-JETSTREAM.md` |

### Servicios que publican eventos

| Servicio | Eventos que publica | ¿Ya tiene código de publicación? |
|----------|---------------------|----------------------------------|
| api-gateway | `usage.recorded`, `request.failed` | ✅ Outbox |
| model-router | `account.marked_unhealthy`, `account.recovered` | ✅ Outbox |
| credential-manager | `account.marked_unhealthy`, `account.recovered` | ✅ Outbox transaccional |
| payment-services | `payment.succeeded`, `payment.failed` | ✅ Outbox transaccional |
| subscription-service | `subscription.changed` | ✅ Outbox transaccional |
| billing-service | `wallet.debited`, `wallet.low_balance`, `billing.period_closed` | ✅ Outbox transaccional/durable |
| key-services | `api_key.revoked` | ✅ Outbox transaccional |
| key-services | `api_key.authorization_changed` | ✅ Outbox transaccional |
| file-service | `file.ingest.requested`, `file.vectors.delete.requested` | ✅ Outbox transaccional |

### Servicios que consumen eventos

| Servicio | Eventos que consume | ¿Ya tiene consumer? |
|----------|---------------------|---------------------|
| billing-service | `payment.succeeded`, `usage.recorded` | ✅ Pull durable + ACK/DLQ |
| usage-service | `usage.recorded` | ✅ Pull durable + ACK/DLQ |
| invoice-service | `billing.period_closed` | ✅ Pull durable + ACK/DLQ |
| api-gateway | `api_key.revoked` | ✅ Pull durable + ACK/DLQ |
| api-gateway | `api_key.authorization_changed` | ✅ Pull durable + invalidación de caché |
| file-service | `file.ingest.requested` | ✅ Pull durable + ACK/reintentos/DLQ |
| file-service | `file.vectors.delete.requested` | ✅ Pull durable + ACK/reintentos/DLQ |

---

## Fase 2 — Build y ejecución del backend

| # | Item | Detalle |
|---|------|---------|
| 2.1 | **Build de producción** | ✅ `build-all.sh` compila los binarios release de los 15 servicios/apps backend. |
| 2.2 | **Ejecución** | ✅ `run-all.sh` ejecuta únicamente binarios release, aplica migraciones y espera dependencias; no inicia contenedores ni frontend. |
| 2.3 | **Docker para microservicios** | No aplica por decisión del proyecto: Docker Compose contiene infraestructura local, no los microservicios Rust. |
| 2.4 | **Health checks reales** | ✅ `run-all.sh` verifica PostgreSQL, Redis, NATS, Qdrant, Vault y disponibilidad de cada servicio iniciado. |

---

## Fase 3 — TLS/mTLS entre servicios

| # | Item | Detalle |
|---|------|---------|
| 3.1 | ✅ **TLS/mTLS multicloud implementado** | `crates/common/src/tls.rs` aplica `TLS_MODE=disabled|tls|mtls`, configuración fail-closed y proveedores `local|aws|azure|vault` exclusivamente para servidores/clientes gRPC entre microservicios. PostgreSQL, Redis, NATS/JetStream y Vault conservan su transporte actual. |
| 3.2 | ✅ **Carga y rotación de certificados** | AWS Secrets Manager, Azure Key Vault y Vault KV/PKI usan bundles por servicio, caché con refresco, validación X.509 y métricas de expiración. Ver `TLS-MTLS-MULTICLOUD.md`. |
| 3.3 | ✅ **Vault PKI/KV local automatizado** | Vault persistente se inicializa, hace unseal y configura CA, rol PKI, KV v2, política y AppRole mediante `vault-bootstrap`. Contraseñas, peppers, tokens y claves se migran fuera de `.env`; ver `VAULT-SECRETS.md`. |
| 3.4 | **Service JWT** | `crates/common/src/service_jwt.rs` tiene autenticación service-to-service por kid. Verificar que cada servicio tenga su secret configurado y los pares kid/secret estén rotados. |

---

## Fase 4 — Observabilidad

| # | Item | Detalle |
|---|------|---------|
| 4.1 | **Prometheus + Grafana** | ✅ Incluidos en Compose con dashboards de servicios, ingestión y almacenamiento. |
| 4.2 | **Tracing distribuido** | ✅ OTEL Collector exporta a Tempo; servicios instrumentados con `tracing` y spans de HTTP/gRPC/NATS. |
| 4.3 | **Alertas** | ✅ Reglas para errores, p95/p99, outbox, DLQ, cuotas, uso anómalo y backpressure. |
| 4.4 | **Logs estructurados** | ✅ JSON vía tracing; Alloy envía logs a Loki. |

---

## Fase 5 — End-to-End Validation

| # | Item | Detalle |
|---|------|---------|
| 5.1 | **Verificar que los 20 paquetes compilan** | ✅ `cargo check --workspace` y Clippy estricto pasan. |
| 5.2 | **Verificar que se inician correctamente** | Con la infraestructura ya disponible, `run-all.sh` verifica las dependencias, migra cada base y arranca únicamente los servicios backend Rust en el host. No inicia contenedores ni frontend. |
| 5.3 | **Flujo completo de prueba** | Probar: register → verify email → login → create API key → use key → ver usage en dashboard → ver invoice. |
| 5.4 | **Tests de integración** | ✅ Hay pruebas reales de aislamiento/deduplicación/cuotas contra PostgreSQL/Redis; ampliar la matriz en staging cuando se requiera. |
| 5.5 | **Load testing** | Implementado en `scripts/phase9_load_test.sh`; ejecución contra endpoints reales queda opt-in en staging. |

---

## Fase 6 — Frontend: preparación producción

| # | Item | Estado actual |
|---|------|---------------|
| 6.1 | **CI/CD del dashboard** | ✅ Ya tiene GitHub Actions. Verificar que funciona con el BFF real. |
| 6.2 | **Dockerfile del dashboard** | ✅ Ya existe multi-stage con standalone output. |
| 6.3 | **CSP Headers** | ✅ Configurados en `next.config.mjs`. |
| 6.4 | **Archivos/RAG** | ✅ Dashboard cliente incluye `/files`, carga directa R2, cuotas, ingestión, búsqueda y reindexado. |
| 6.5 | **Admin storage** | ✅ Admin dashboard consulta capacidad, documentos, vectores, ingestión y DLQ. |

---

## Fase 7 — Documentación y Runbooks

| # | Item | Detalle |
|---|------|---------|
| 7.1 | **Runbook de deployment** | Paso a paso para deploy de un nuevo release (build → tag → push → deploy). |
| 7.2 | **Runbook de incidentes** | Qué hacer si: BFF caído, DB lenta, Redis lleno, NATS sin espacio, outbox backlog, DLQ acumulándose. |
| 7.3 | **Diagrama de arquitectura actualizado** | El `01-arquitectura-microservicios.md` existe pero puede estar desactualizado vs código. |
| 7.4 | **Playbook de onboarding** | Cómo un nuevo developer clona, configura `.env`, levanta infra, corre tests, y deploya. |

---

## Estado de salida

```
Implementado en código:
  Fase 0 — Vault y secretos
  Fase 1 — Outbox → NATS JetStream
  Fase 2 — build-all/run-all sin Docker para microservicios
  Fase 3 — TLS/mTLS entre servicios
  Fase 4 — Observabilidad
  Fase 6 — Frontends cliente/admin

Validación operativa pendiente en staging:
  Fase 5 — flujo end-to-end con dependencias reales
  Fase 9 — interoperabilidad S3, carga, restauración y smoke PayPal/dunning
```

## Criterio de cierre del roadmap

Una fase solo se marca como cerrada cuando coinciden estas tres condiciones:

1. La funcionalidad existe en código, migraciones y scripts de despliegue.
2. La prueba correspondiente se ejecutó contra dependencias reales o un
   entorno de staging equivalente; no se acepta una afirmación manual.
3. `scripts/verify_fase*.sh` generó el JSON con métricas, `latest.json` y el
   log bajo `artifacts/verify/<fase>/`.

Los pendientes de Fase 5 y Fase 9 son validaciones operativas explícitas. No
implican que falten los servicios, contratos o lógica base.

## Ya está listo (no tocar)

- ✅ customer-bff y admin-bff (tests y builds validados)
- ✅ Dashboards cliente y admin con archivos/RAG y administración de storage
- ✅ Infraestructura base (PostgreSQL multi-base, pgAdmin, Redis/RedisInsight, NATS, Qdrant y Vault)
- ✅ Protos gRPC y stubs generados para los contratos actuales
- ✅ Tipos de eventos (`crates/events`, 14 subjects definidos)
- ✅ Outbox → NATS JetStream (PubAck, deduplicación, pull consumers, ACK/NAK y DLQ)
- ✅ Arquitectura por bounded contexts; la estructura física puede variar por servicio
- ✅ Migraciones SQL por base independiente, incluyendo `files_db` y pipeline RAG
- ✅ JWT service-to-service (`crates/common/src/service_jwt.rs`)
- ✅ Auditoría firmada (`crates/common/src/audit_signer.rs`)
- ✅ Rate limiting por IP y usuario (customer-bff)
