# AI Gateway — Reglas de Ingeniería (AI_CODING_RULES.md)

> Reconciliado el 2026-07-25 con `AGENTS.md`, los 20 paquetes actuales,
> R2, Qdrant, OpenRouter/Qwen3, PayPal, Vault y la observabilidad
> OTEL/Loki/Tempo.

> Adaptado de un manifiesto para motores HTAP de ultra-bajo nivel. Aquí solo
> se conservan los principios que aportan valor real a un servicio de red
> I/O-bound sobre Postgres/Redis/NATS/Bedrock/OpenRouter — no a un motor de
> almacenamiento distribuido propio. La sección 8 explica qué se descartó
> y por qué, para que quede documentado y no se reintroduzca "porque sonaba
> pro" en una futura iteración.

## 1. Cero explicaciones genéricas, razonamiento explícito antes de código complejo

Antes de implementar un caso de uso no trivial (ej. la reserva optimista de
saldo, el algoritmo de selección de cuenta AWS, el circuit breaker), el
agente debe escribir un bloque breve explicando:
- qué invariante de negocio protege el diseño (ej. "el balance nunca puede
  quedar negativo aunque lleguen 50 requests concurrentes de la misma key")
- qué pasa si la operación se ejecuta dos veces (idempotencia)
- qué pasa si la operación se corta a la mitad (crash entre el débito y la
  llamada a Bedrock)

No se permite código complejo sin este razonamiento previo.

## 2. Observabilidad obligatoria

- **Cero `println!`/`dbg!`** en ningún servicio. Todo pasa por `tracing`.
- Cada request HTTP en `api-gateway` genera un `request_id` (UUID) que se
  propaga como metadata en: el gRPC interno a `key-services`/`model-router`,
  el evento `usage.recorded` en NATS, y los logs de `billing-service` que
  lo procesan. Debe poder trazarse una petición completa, de punta a punta,
  filtrando por ese único ID.
- Exportación a OpenTelemetry desde el día uno (no "lo agregamos después").
- Cero configuración mágica: timeouts, tamaños de pool, límites de
  reintentos, todo se carga desde variables de entorno, con `.env.example`
  documentado por servicio.

## 3. Contratos internos estrictos

- **Cero JSON entre microservicios internos.** La comunicación síncrona
  (`api-gateway` ↔ `key-services`/`model-router`/`credential-manager`) usa
  gRPC con esquemas `.proto` versionados en `crates/proto`.
- Los eventos de NATS usan un **envelope canónico** inmutable definido en
  `crates/events`: `event_id`, `event_type`, `schema_version`,
  `occurred_at`, `payload`. Nunca se construye un `serde_json::json!` ad-hoc
  dentro de un handler.
- Cualquier cambio de campo en un `.proto` o en un evento es un cambio de
  versión explícito (`schema_version` incrementado), nunca un cambio
  "silencioso" que rompe al consumidor.

## 4. Zero Trust entre servicios y aislamiento entre usuarios

- Ningún microservicio interno confía en una llamada solo por "venir de la
  VPC". Autenticación servicio-a-servicio obligatoria (mTLS o JWT de
  servicio rotativo) en todo endpoint gRPC interno.
- **`user_id` es un parámetro obligatorio, nunca opcional**, en cada trait de
  repositorio que toque datos de negocio (`usage_events`, `wallets`,
  `api_keys`, etc.). Prohibido cualquier query que pueda, por error de
  lógica, devolver o mutar filas de un usuario distinto al
  solicitado.
- Los tests de cada repositorio deben incluir explícitamente un caso que
  verifique que una consulta con `user_id = A` nunca devuelve filas de
  `user_id = B`, aunque ambas existan en la misma tabla/partición.

## 5. Definition of Done sin concesiones

- Prohibido `// TODO: implementar después`, `// omitido por brevedad`, o
  cualquier variante. Si algo no se implementa en esta iteración, se declara
  explícitamente como pendiente en el roadmap (`ROADMAP-PRODUCCION.md`), no se deja como comentario fantasma en el
  código.
- Manejo real de error en **cada** llamada externa: timeout y `429` de
  Bedrock/OpenRouter, conexión perdida de Postgres, Redis caído, NATS sin ack. Un
  `match` que solo cubre el caso feliz y un `_ => unreachable!()` no pasa
  revisión.
- Cada gate de verificación (`scripts/verify_fase*.sh`) genera artefactos en
  `artifacts/verify/<fase>/`: `<timestamp>.json` (reporte inmutable),
  `latest.json` (puntero al último resultado) y `<timestamp>.log`. Ninguna
  fase se marca como cerrada sin estas tres evidencias con métricas reales
  (duración, tests passed/failed, etc.), no con una afirmación de texto del
  agente.

## 6. Testing de resiliencia (chaos, adaptado a un sistema de billing)

No estás construyendo Raft ni tu propio motor de consenso, así que no
necesitas Crash/Partition/Latency Monkey sobre un clúster propio. Lo que sí
es no negociable, porque involucra dinero real de clientes:

- **Cero tests basados en `sleep()`** para esperar propagación de eventos.
  Usa implementaciones deterministas de `EventPublisher`/`Clock` o una base
  real de prueba con espera activa por
  condición, nunca por tiempo fijo.
- **Prueba de redelivery de NATS:** NATS JetStream es *at-least-once*.
  Todo consumer de `billing-service` debe tener un test que simule recibir
  el **mismo** `usage.recorded` dos veces y verifique que el `wallet_ledger`
  solo registra el cargo una vez (vía `idempotency_key`).
- **Prueba de corte a mitad de operación:** simular que el proceso muere
  entre "reservar saldo" y "confirmar el débito real" — al reiniciar, debe
  existir un mecanismo (job de reconciliación o expiración de reserva) que
  libere reservas huérfanas, nunca dejarlas bloqueando saldo para siempre.
- **Verificación de cero pérdida/duplicidad de dinero:** después de
  cualquier test de caos, el `balance` en `wallets` debe coincidir
  exactamente con la suma reconstruida desde `wallet_ledger` — si no
  coincide, el test falla, sin excepciones.
- Property-based testing (`proptest`) para lógica pura de dominio con
  muchos casos borde: cálculo de costo a partir de tokens y pricing,
  validación de invariantes de una `ApiKey` o un `Wallet`. Nada de
  `let payload = "test"` quemado para esto.

## 7. Métricas por percentiles, no promedios

- Todo endpoint crítico expone histogramas de latencia (Prometheus), nunca
  solo un promedio. El dashboard interno debe mostrar p50/p90/p99/p99.9.
- Presupuestos de latencia por etapa (ajustar con benchmarks reales, esto es
  punto de partida):

  | Etapa | Presupuesto p99 |
  |---|---|
  | Validación de API key (`key-services`, con cache Redis) | < 5 ms |
  | Selección de cuenta AWS (`model-router`) | < 10 ms |
  | Overhead propio total (sin contar la llamada al modelo) | < 50 ms |
  | Publicación del evento `usage.recorded` a NATS | < 5 ms |

- Si un cambio arquitectónico rompe uno de estos presupuestos, se revierte o
  se re-diseña — no se acepta "ya lo optimizamos después".
- El foco de optimización real está en: evitar N+1 queries a Postgres,
  evitar locks largos en Redis, cachear agresivamente la validación de
  key/plan. **No** perseguir cero-allocations en el hot path: la latencia
  dominante de este sistema son los cientos de milisegundos de la llamada al
  modelo, no microsegundos de acceso a memoria.

## 8. Reglas del manifiesto original que se descartan explícitamente (y por qué)

| Regla original | Por qué no aplica aquí |
|---|---|
| `io_uring` / `O_DIRECT`, bypass de caché del kernel | No hay NVMe propio: el almacenamiento es Postgres/Redis gestionados. |
| SIMD explícito, pivote AOS→SOA (Arrow) | No hay motor OLAP en memoria propio; las agregaciones viven en vistas materializadas de Postgres. |
| Lock-free hot path con `crossbeam-epoch`/RCU | No hay una hot cache en memoria compartida entre hilos que lo justifique; Redis ya resuelve la coordinación de estado compartido. |
| Consenso Raft propio | No construyes tu propio almacenamiento distribuido; usas Postgres. |
| Protocolo binario propio sobre TCP crudo, multiplexación Reactor manual | gRPC/HTTP2 ya te da multiplexación y conexión persistente sin reinventar el protocolo. |
| `#[repr(align(64))]` obligatorio contra false sharing, `jemalloc`/`mimalloc` obligatorio | Sin hot loops en memoria compartida de alta frecuencia, el allocator por defecto de Rust es suficiente; no vale la complejidad. |
| "Cero allocations" verificado con `dhat-rs` en el hot path | La latencia del sistema está dominada por I/O de red hacia Bedrock/Postgres, no por el heap. |
| Pacto de `unsafe` con prueba `// SAFETY:` obligatoria | Aplica **solo si** en algún punto se justifica un bloque `unsafe` (poco probable en un servicio CRUD/proxy). Si aparece, la regla se reactiva tal cual — se deja documentada por si acaso, no se borra del todo. |

## 9. Reglas de disciplina de ingeniería para agentes y desarrolladores

```
Aplica además estas reglas de disciplina de ingeniería en todo el proyecto:

1. Prohibido println!/dbg!; toda instrumentación va por tracing +
   OpenTelemetry, con un request_id propagado desde api-gateway hasta los
   eventos de NATS y los logs de billing-service.
2. Comunicación interna síncrona solo por gRPC (nunca JSON ad-hoc); eventos
   de NATS solo con el envelope canónico de crates/events.
3. user_id es obligatorio y no-opcional en cada método de repositorio que
   toque datos de negocio; escribe un test por repositorio que verifique
   aislamiento entre usuarios.
4. Prohibido dejar TODOs o casos de error sin manejar en llamadas externas
   (Bedrock/OpenRouter, PayPal, Postgres, Redis, NATS, R2 y Qdrant).
5. Todo consumer de eventos de billing debe tener un test que simule
   redelivery duplicado del mismo evento y verifique que el cargo no se
   duplica (idempotency_key).
6. No uses sleep() en tests para esperar propagación de eventos; usa relojes y
   publishers deterministas sin mocks.
7. Expón métricas de latencia como histogramas (p50/p90/p99/p99.9), nunca
   como promedio, en cada endpoint del hot path.
8. No implementes ninguna de las técnicas de bypass de kernel/SIMD/Raft
   propio del manifiesto original: no aplican a este sistema y agregan
   complejidad sin beneficio medible aquí.
  Está prohibido usar mocks.
Está prohibido devolver código parcial.
Está prohibido usar todo!(), panic!(), unimplemented!().
Cada función debe quedar completa.
Si una implementación requiere 2.000 líneas, genera las 2.000 líneas.

Si no puedes cumplir todas las reglas del documento, detente y explica qué información falta. No inventes código, no uses mocks, no escribas TODOs, no simplifiques la implementación.



Antes de cerrar un servicio, verifica estos 8 puntos contra el código
generado igual que verificas los 10 puntos de seguridad.
```
