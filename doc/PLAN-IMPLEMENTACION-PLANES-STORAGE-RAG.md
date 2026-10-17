# Plan de implementación: planes, almacenamiento y RAG

## 1. Objetivo

Última actualización de implementación: 2026-07-25.

Implementar la oferta comercial de Nexus basada en tres niveles de acceso a modelos de IA, almacenamiento de archivos, embeddings y búsqueda RAG. La identidad de propiedad es siempre `user_id`; el sistema no utiliza `tenant_id` ni organizaciones.

El diseño debe garantizar:

- Aislamiento estricto entre usuarios.
- Cuotas verificadas en backend, nunca en el frontend.
- Cobro y bloqueo coherentes con el estado de la suscripción.
- Trazabilidad completa de archivos, vectores, consumo y cobros.
- Eliminación/anonymización segura sin romper registros contables.

### Decisión de proveedor de embeddings

Se utilizará exclusivamente OpenRouter como API OpenAI-compatible con
`qwen/qwen3-embedding-8b`. Cohere y cualquier otro proveedor gestionado quedan
fuera del alcance. Los planes se diferencian por cuotas, almacenamiento,
concurrencia y límites de uso; no por cambiar de proveedor.

## Estado de implementación

| Fase | Estado | Nota |
|---|---|---|
| Fase 0 — Contrato y decisiones | Completada | Contrato inicial definido: Starter 5 USD, Pro 10 USD, Pro Plus 20/75 USD según modalidad, cuotas y capacidades documentadas. |
| Fase 1 — Modelo de planes y límites | Completada | Migraciones, Starter/Pro/Pro Plus, límites de almacenamiento/RAG, facturación, contrato gRPC, persistencia, cache e interfaz Admin BFF validados. |
| Fase 1A — Cumplimiento legal | Completada | Aceptación versionada persistida, idempotente y obligatoria antes de crear suscripciones. |
| Fase 2 — Suscripción y cobro | Completada | PayPal checkout, captura idempotente, Vault recurrente, renovación automática, dunning, gracia y bloqueo validados. |
| Fase 3 — Servicio de archivos | Implementada; validación operativa pendiente | Bloques 1–4 están implementados: autorización, cuotas, integración R2, API de `customer-bff` y `s3-gateway` compatible con S3/SigV4, multipart, streaming y rangos. Falta ejecutar interoperabilidad con clientes S3 externos y el endurecimiento operativo final; por eso no se marca cerrada. |
| Fase 4 — Ingestión y embeddings | Completada (4A + 4B + 4C) | Outbox transaccional, relay a JetStream, consumidor durable con ACK/reintentos/DLQ, extracción PDF/DOCX/XLSX/TXT, chunking Qwen versionado por tokens, cliente OpenRouter endurecido, persistencia de tokens/costo/estado, Qdrant, deduplicación canónica y reindexado incremental referencial. |
| Fase 5 — Qdrant | Implementada; validación de cierre pendiente | Topología canónica, aislamiento obligatorio, resolución PostgreSQL, límites de ingestión por plan, reserva Redis/PostgreSQL, backpressure JetStream y administración desde Admin BFF están implementados. Falta ejecutar el gate específico de cierre de Fase 5 con sus dependencias reales. |
| Fase 4A — Qwen/OpenRouter | Completada | Tokenizer oficial fijado por SHA, contexto máximo, chunking real por tokens, batching concurrente, reintentos con Retry-After/backoff, costo real, instrucción de consulta y validación de dimensiones. |
| Fase 4B — Deduplicación | Completada | Caché canónica aislada por usuario/modelo/versión/hash, leases concurrentes, referencias independientes, métricas de ahorro, búsqueda relacional y borrado al quedar sin referencias. |
| Fase 4C — Reindexado incremental | Completada | Diff transaccional por posición/estrategia/modelo/versión; referencias estables para chunks sin cambios, actualización selectiva de chunks modificados, eliminación de referencias obsoletas, cola de huérfanos y telemetría persistida. |
| Fase 5A — Topología Qdrant | Completada | Starter/Pro/Pro Plus usan colección compartida por defecto; la dedicada solo se activa mediante `features.dedicated_qdrant_enabled=true`, nunca por slug o precio. Payload canónico, filtros obligatorios y resolución SQL validados. |
| Fase 8A — Pruebas adversariales | Completada | Matriz adversarial, pruebas de aislamiento/deduplicación/reintentos y observabilidad de ingestión, OpenRouter, JetStream y Qdrant. |
| Fase 9A — Migración y despliegue | Completada | Migraciones aditivas, backfill/compare auditados, dual-write/read v1-v2, corte gradual y cleanup protegido por backup. |
| Fase 6 — API keys y autorización | Completada | Scopes independientes de modelos/archivos, límites dinámicos `max_api_keys` por suscripción, rate limits separados para creación/rotación, rotación con preflight y sin doble consumo del bucket de creación, edición de scopes con invalidación, epochs Redis compartidos con subscription-service y purga L1 del API Gateway ante cambios de suscripción. R2/S3 exige `files:read|write|delete`; modelos exigen `inference:write`; filtros de propietario no se derivan de la key. Migración aditiva de scopes permitidos aplicada. |
| Fase 7 — Frontends | Completada | Dashboard cliente `/files` con explorador, carga directa R2 con progreso, estados de ingestión, descarga firmada, renombrado, borrado, reindexado, cuota de almacenamiento/vectorial, búsqueda RAG, API keys/scopes y suscripción/pagos/consumo. Admin Dashboard `/storage` con capacidad por usuario, documentos/carpetas, colección/vectorización, estados de ingestión y profundidad real de `EVENTS_DLQ`; acciones de plan y bloqueo existentes conservan auditoría en Admin BFF. Builds de ambos frontends y `cargo check/clippy -p admin-bff` validados. |
| Fase 8 — Observabilidad y seguridad | Completada | Trazas de subida/ingestión/Qdrant/RAG, métricas con `user_hash`/plan/resultado, auditoría durable, alertas operativas, aislamiento defensivo y pruebas reales de PostgreSQL/Redis para cruces y carreras. |
| Fase 8B — Configuración administrable en caliente | Planificada | Diseño de configuración dinámica desde Admin BFF; no incluye secretos, red, TLS ni endpoints de infraestructura. Requiere migraciones, autorización, auditoría, caché e invalidación antes de marcarse implementada. |
| Fase 9 — Rendimiento y producción | Implementada; gate estático aprobado | Backpressure configurable, scripts de carga p50/p95/p99, reintentos idempotentes, backup/restore, recuperación de Qdrant y smoke de pagos/dunning están implementados. `artifacts/verify/fase9/latest.json` valida sintaxis, tests, check, Clippy y Compose; las pruebas live y restauraciones destructivas siguen opt-in y no se ejecutaron en ese gate. |

### Validación técnica actual

- `cargo check --workspace`: correcto.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  correcto, sin `#[allow]` en `apps/` ni `crates/`.
- `cargo test --workspace`: la suite reproduce el fallo ambiental previamente
  conocido de DNS en `common::ssrf::tests::allows_public_https`.
- Las pruebas de integración de outbox requieren `DATABASE_URL` y servicios
  PostgreSQL/NATS disponibles; sin esas dependencias fallan por configuración,
  no por compilación.
- `client-platform-dashboard`: `bun run build` y `bun run test` correctos
  (49 pruebas).
- `admin-plataform-dashboard`: `bun run build` y `bun run test` correctos
  (35 pruebas).
- `admin-bff`, `customer-bff` y `file-service`: `cargo check`, Clippy estricto
  y compilación de los contratos gRPC correctos.
- `scripts/verify_fase8.sh`: correcto; reporte y log inmutables generados en
  `artifacts/verify/fase8/20260725T051637Z.*`. El gate ejecutó las pruebas
  reales de aislamiento PostgreSQL, deduplicación canónica y carreras de cuota
  contra PostgreSQL/Redis locales, además de Clippy estricto.
- El contrato `FileService` incluye `GetStorageUsage`; la cuota usada/restante
  se calcula autoritativamente desde `file_quotas`. `VectorUsageResponse`
  también expone tokens, costo en micros, tokens evitados y trabajos completados
  o fallidos para que ambos dashboards no calculen consumo localmente.

### Reconciliación de estado — 2026-07-25

Esta matriz corrige el desfase entre implementación y documentación. `Completada`
significa que existe implementación y evidencia de validación asociada;
`Implementada; validación pendiente` significa que el código existe, pero aún
falta ejecutar una prueba operativa o de interoperabilidad antes de declarar el
cierre de producción.

- Fases 0, 1, 1A, 2, 4, 4A, 4B, 4C, 5A, 6, 7 y 8: implementación documentada y validaciones registradas en sus secciones.
- Fase 3: `file-service`, `customer-bff` y `s3-gateway` existen y compilan; queda pendiente la prueba con clientes S3 externos y el endurecimiento operativo.
- Fase 5: topología, aislamiento, cuotas, referencias canónicas y endpoints existen; queda pendiente un gate específico de cierre con PostgreSQL, Redis y Qdrant reales.
- Fase 8A: gate real registrado en `artifacts/verify/fase8/latest.json`, incluyendo pruebas de PostgreSQL/Redis.
- Fase 9: gate registrado en `artifacts/verify/fase9/latest.json`; las cargas HTTP live, restauraciones y smoke de PayPal/dunning se deben ejecutar explícitamente en un entorno de validación.

Mientras existan esos pendientes, el plan completo no debe anunciarse como
cerrado al 100%.

## 2. Oferta comercial propuesta

### Starter — 5 USD

Prueba de 3 días. Se solicita y verifica el método de pago antes de comenzar.

- El checkout debe mostrar claramente: monto, moneda, fecha exacta del cobro y renovación.
- Cobro automático de 5 USD al finalizar el día 3 si el usuario no cancela.
- Recordatorio por email 24–48 horas antes del cobro, sujeto a la jurisdicción aplicable.
- Cancelación inmediata desde el dashboard, sin ticket de soporte.
- 10 GB de almacenamiento por usuario.
- Embeddings habilitados.
- Qdrant compartido con filtro obligatorio por `user_id`.
- 100.000 tokens durante la prueba.
- 1 API key.
- Modelos básicos y rate limit bajo.
- Métricas básicas.
- RAG para PDF, DOCX, XLSX y TXT.

### Pro — 10 USD/mes

Cobro desde el primer día, antes de habilitar el uso.

- 25 GB de almacenamiento por usuario.
- Embeddings habilitados.
- Qdrant compartido aislado por `user_id`.
- 10 millones de tokens mensuales.
- API keys sin límite numérico (`max_api_keys = 0`), sujetas a rate limit de creación y rotación.
- Modelos avanzados.
- RAG para PDF, DOCX, XLSX y TXT.
- Rate limits altos.
- Métricas detalladas.
- Sobreconsumo configurable y visible antes de cobrarlo.

### Pro Plus — 20 USD/mes

Cobro desde el primer día, antes de habilitar el uso.

- 100 GB de almacenamiento por usuario.
- Embeddings habilitados.
- Qdrant compartido aislado por `user_id`.
- Uso generoso sujeto a política de uso justo; no prometer ilimitado sin límites operativos.
- API keys sin límite numérico (`max_api_keys = 0`), sujetas a rate limit de creación y rotación.
- Modelos avanzados.
- RAG para PDF, DOCX, XLSX y TXT.
- Rate limits altos y mayor concurrencia que Pro.
- Métricas detalladas.
- Sobreconsumo configurable.

### Pro Plus — 75 USD/mes

- 1 TB de almacenamiento por usuario.
- Embeddings y Qdrant dedicado opcional mediante la capacidad contractual explícita `dedicated_qdrant_enabled=true`.
- La modalidad de 75 USD no activa Qdrant dedicado automáticamente.
- Límites de tokens y concurrencia personalizados.
- Acceso a todos los modelos habilitados.
- RAG avanzado.
- Explorador de archivos, carpetas y gestión desde el frontend.
- URL de acceso controlado y API compatible con S3.
- Auditoría, SSO, MFA, SLA y soporte prioritario.
- Retención y residencia de datos configurables.
- Facturación contractual.

Este es el plan más avanzado del catálogo. Las capacidades empresariales
(SSO, SLA, soporte prioritario, residencia de datos, mTLS, redes permitidas y
facturación contractual) se habilitan dentro de Pro Plus mediante configuración
o add-ons contractuales. No existe un cuarto plan Enterprise separado.

## 3. Términos y condiciones

### Rutas públicas

El frontend debe publicar como mínimo:

- `https://jnmtortas.com/terms` — términos y condiciones.
- `https://jnmtortas.com/privacy` — política de privacidad.
- `https://jnmtortas.com/cookies` — política de cookies, si aplica.

El Admin Dashboard debe publicar las mismas políticas bajo su dominio si el
flujo administrativo requiere aceptación:

- `https://admin.jnmtortas.com/terms`.
- `https://admin.jnmtortas.com/privacy`.

### Aceptación obligatoria

- Registrar aceptación antes de activar una suscripción o iniciar una prueba.
- Mostrar la versión exacta de los documentos aceptados.
- Guardar `user_id`, versión de términos, versión de privacidad, timestamp,
  IP, user-agent y país si está disponible.
- No aceptar términos desde un checkbox ya marcado.
- Para Starter, registrar la aceptación antes de iniciar los 3 días de prueba.
- Para Pro y Pro Plus, registrar la aceptación antes del primer cobro.
- Solicitar una nueva aceptación cuando cambie una versión material.

### Modelo de datos y API

Crear una tabla propia de cumplimiento, por ejemplo:

```text
terms_acceptances
-----------------
id
user_id
terms_version
privacy_version
accepted_at
ip_address
user_agent
country
```

El backend debe exponer una operación idempotente para registrar la aceptación
y una consulta para saber si el usuario tiene la versión vigente aceptada. La
activación de prueba, suscripción y método de pago debe rechazar el flujo si la
aceptación no existe.

## 4. Reglas de facturación y suscripciones

1. El checkout debe declarar antes del pago: precio, moneda, fecha de cobro, renovación, prueba y política de cancelación.
2. Para Starter, ejecutar una preautorización o verificación del método de pago antes de activar la prueba. La preautorización no debe presentarse como cobro.
3. Enviar recordatorio previo al cobro según las obligaciones legales aplicables.
4. La cancelación debe ser una acción directa desde el dashboard.
5. Implementar dunning:
   - Reintentos con backoff.
   - Notificación de fallo.
   - Estado `past_due`.
   - Período de gracia configurable.
   - Bloqueo progresivo al finalizar la gracia.
6. Definir moneda por proveedor. USD estricto debe usar un gateway que cobre USD directamente; Wompi/PayU deben mostrar COP y la conversión aplicada.
7. Todo importe, descuento, impuesto, moneda y referencia del proveedor debe quedar en una factura inmutable.

## 5. Propiedad y aislamiento de datos

Cada objeto debe estar ligado a un usuario:

```text
user_id
document_id
file_id
collection_id
```

Reglas obligatorias:

- Nunca aceptar `user_id` de propiedad desde el body del cliente.
- Obtener el usuario desde la sesión/API key validada por el backend.
- Aplicar filtro `user_id` en cada búsqueda Qdrant.
- Verificar pertenencia antes de leer, modificar, mover o borrar cualquier archivo.
- No mezclar documentos entre usuarios.
- Guardar originales en R2 bajo una ruta privada, por ejemplo `users/{user_id}/files/{file_id}`.
- Usar URLs firmadas con expiración para descargas y subidas.
- Las API keys de LLM no deben conceder automáticamente acceso a archivos.
- Separar scopes: `models:invoke`, `files:read`, `files:write`, `vectors:query` y `vectors:write`.

## 6. Fases de implementación

### Fase 0 — Contrato y decisiones

- Congelar nombres, precios, moneda y política de uso justo.
- Definir límites técnicos por plan: tamaño máximo de archivo, archivos diarios, concurrencia de ingestión, RPM/TPM y cuota mensual.
- Definir proveedores de pago habilitados por moneda.
- Versionar los contratos Protobuf y eventos.

### Fase 1 — Modelo de planes y límites

- Añadir a `plans`:
  - `monthly_token_quota`.
  - `storage_bytes_quota`.
  - `max_file_size_bytes`.
  - `max_files`.
  - `max_api_keys`.
  - `embeddings_enabled`.
  - `rag_enabled`.
  - `qdrant_mode`.
  - `rpm`, `tpm` y `max_concurrent`.
  - `fair_use_policy`.
- Exponer `GetPlanLimits` en `subscription-service`.
- Cachear límites en Redis e invalidarlos al modificar un plan.
- Aplicar límites en `api-gateway`, `key-services` y los servicios de archivos.

#### Entrega completada

- Migraciones idempotentes `20260724000100`, `20260724000200` y `20260724000300`.
- Capacidades comerciales persistidas como columnas tipadas: precio, moneda, prueba,
  sobreconsumo, política de uso justo, gracia, SSO/SLA/mTLS/S3, almacenamiento,
  archivos, embeddings, RAG y modo Qdrant.
- Starter, Pro y Pro Plus sembrados con cuotas concretas y Pro Plus como nivel más
  avanzado (incluye S3 y Qdrant dedicado opcional).
- Contrato Protobuf y `GetPlanLimits` exponen todas las capacidades; Redis conserva
  e invalida los límites al cambiar la suscripción.
- Administración de planes (crear/actualizar/desactivar) transmite y valida los
  campos explícitos desde Admin BFF hasta PostgreSQL.
- Validación de backend impide precios/cuotas negativos, monedas inválidas, modos
  Qdrant desconocidos y S3 sin una modalidad Qdrant.
- Verificación realizada: `SQLX_OFFLINE=true cargo check --workspace`, pruebas de
  `subscription-service`/`admin-bff` y `bun run build` del Admin Dashboard.

### Fase 1A — Cumplimiento legal

- Crear rutas `/terms`, `/privacy` y `/cookies`.
- Versionar los documentos legales.
- Crear `terms_acceptances` en una base de datos con auditoría.
- Añadir aceptación obligatoria al registro, checkout, prueba y cambio de plan.
- Implementar reaceptación cuando cambie una versión material.

#### Entrega completada

- `terms_acceptances` persiste usuario, versiones, timestamp, IP, user-agent y país,
  con clave única para aceptación idempotente.
- `AcceptTerms` y `GetTermsStatus` están disponibles en `subscription-service`.
- Customer BFF expone `POST /api/v1/legal/accept`.
- Crear una suscripción rechaza versiones ausentes/desactualizadas o sin aceptación
  persistida.
- El checkout exige checkbox explícito y enlaza términos y privacidad antes de
  registrar la aceptación.

### Fase 2 — Suscripción y cobro

#### Entrega inicial completada

- Starter inicia en estado `trialing` durante los 3 días definidos por el plan.
- Pro/Pro Plus inician en `active`.
- La suscripción conserva `trial_ends_at`, `grace_period_ends_at`, contador de
  reintentos, último error y referencia del proveedor.
- Un proceso de ciclo de vida revisa cada minuto la expiración de pruebas y
  períodos de gracia; las pruebas vencidas pasan a `past_due` y, agotada la
  gracia, se bloquean como `canceled`.
- Los límites solo se resuelven para suscripciones `trialing` o `active`, por lo
  que `past_due` queda sin acceso automáticamente.
- La transición manual `MarkSubscriptionPastDue` registra reintento, error y
  gracia de tres días.
- Persistencia y contrato gRPC exponen el estado completo del ciclo de vida.
- Checkout de planes de pago crea una orden PayPal real; el retorno captura la
  orden y solo después activa la suscripción.
- Capturas repetidas son idempotentes mediante eventos y referencias únicas.
- El webhook PayPal valida la firma, deduplica eventos y actualiza la transacción
  con eventos de éxito/fallo.
- PayPal Vault permite cargos recurrentes con el método predeterminado del usuario;
  el job de renovación ejecuta cargos vencidos cada minuto con clave idempotente.
- Los fallos de renovación reintentan hasta tres veces, conservan el motivo,
  mantienen una gracia de tres días y después bloquean el acceso.
- Tras un cargo exitoso se renueva el ciclo, se limpia el estado de dunning y se
  invalida la caché Redis de límites.

La integración se mantiene exclusivamente con PayPal. El cargo recurrente usa
PayPal Vault y requiere un método PayPal predeterminado; si falla, se reintenta
hasta tres veces durante la gracia y luego se bloquea la suscripción. No se
simulan cobros exitosos ni se aceptan proveedores alternativos.

- Implementar Starter con prueba de 3 días.
- Implementar preautorización/verificación del método de pago.
- Implementar cobro automático y renovación.
- Implementar dunning, período de gracia y bloqueo.
- Implementar cancelación inmediata y al final del ciclo.
- Emitir eventos `subscription.changed`, `billing.payment_failed` y `billing.subscription_blocked`.
- Registrar auditoría de cada transición.

### Fase 3 — Servicio de archivos

#### Bloque 1 — `file-service` (completado)

- Servicio gRPC independiente en `apps/file-service`, escuchando en `50071` y
  protegido por el mismo esquema de JWT de servicio y TLS/mTLS del resto del
  backend.
- Base de datos PostgreSQL propia (`files_db`) provisionada por Vault y añadida
  al bootstrap local; el servicio no comparte tablas con ningún BFF.
- Migraciones para `files`, `folders`, `file_quotas` y `file_audit_log`, con
  índices por `user_id`, estados controlados (`pending`, `processing`, `ready`,
  `failed`, `deleted`) y borrado lógico.
- `CreateFile` es idempotente por `(user_id, idempotency_key)` y reserva cuota
  dentro de la misma transacción; las carreras concurrentes no pueden exceder
  `quota_bytes` ni `max_files`.
- `CreateFolder`/`ListFolders`, `CreateFile`/`ListFiles`, confirmación y borrado
  lógico y transición controlada de estado (`processing`, `ready`, `failed`)
  están disponibles por gRPC. Se valida ownership de carpetas antes de
  asociarlas a un archivo.
- Cada creación, confirmación, borrado y creación de carpeta deja auditoría
  inmutable con `user_id`, recurso, acción y metadata.
- `build-all.sh` compila el binario de producción y `run-all.sh` ejecuta el
  binario release, aplica migraciones y espera la salud del puerto `50071`.
- La cuota `0` representa sin límite durante esta etapa; el planificador de
  suscripciones podrá actualizar `file_quotas.quota_bytes` y `max_files` por
  usuario sin cambiar el contrato del servicio.

#### Bloque 2 — Autorización y cuotas (completado)

- Cada operación del servicio resuelve el plan vigente mediante
  `PlanResolutionService/GetPlanLimits`; no se permite operar sin suscripción
  activa.
- Se aplican `storage_bytes_quota`, `max_file_size_bytes` y `max_files` del plan
  antes de crear o reservar un archivo. Los límites se sincronizan por usuario
  para reflejar cambios de plan.
- La reserva de bytes y cantidad de archivos ocurre dentro de una transacción
  con actualización condicional; dos subidas concurrentes no pueden exceder la
  cuota.
- Todas las operaciones vuelven a validar la suscripción y filtran por
  `user_id`; las carpetas padre y las carpetas asignadas a archivos también
  verifican ownership en backend.
- Se añadieron scopes de API key separados: `files:read`, `files:write` y
  `files:delete`. El servicio de keys los valida y el gateway conserva los
  scopes validados para su enforcement en las rutas de archivos/S3.

#### Bloque 3 — Integración R2 (completado)

- `file-service` usa el endpoint S3-compatible de Cloudflare R2 con credenciales
  cargadas desde Vault (reutiliza las credenciales R2 ya gestionadas; nunca se
  envían al navegador).
- `CreateUploadUrl` genera una URL `PUT` prefirmada con expiración limitada,
  `Content-Type` y `x-amz-meta-sha256`; el tamaño se verifica contra el objeto
  real para mantener compatibilidad con navegadores que no permiten fijar
  manualmente `Content-Length`.
- `customer-bff` expone el flujo HTTP completo para crear/listar, solicitar URL,
  subir directamente al R2, confirmar, descargar, renombrar, mover y eliminar:
  `/api/v1/files`, `/api/v1/files/{id}/upload-url`,
  `/api/v1/files/{id}/download-url`, `/api/v1/files/{id}/confirm` y
  `/api/v1/files/{id}`.
- La gestión de carpetas incluye crear/listar, renombrar, mover y eliminar de
  forma lógica mediante `/api/v1/folders` y `/api/v1/folders/{id}`; el backend
  valida ownership, evita borrar carpetas no vacías y registra auditoría.
- La confirmación ejecuta `HEAD` y descarga el objeto para comprobar tamaño,
  SHA-256 real y MIME detectado; una discrepancia marca el archivo como
  `failed` y registra auditoría.
- El borrado de usuario es lógico y libera la cuota. El borrado físico solo
  está disponible mediante `PurgeFile`, exige caller `admin-bff`, requiere que
  el archivo ya esté en `deleted`, elimina el objeto de R2 y registra auditoría.
- El servicio valida el bucket al iniciar y no arranca en estado aparentemente
  sano si R2 no está disponible.

#### Bloque 4 — `s3-gateway` Pro Plus (implementado en código; interoperabilidad pendiente)

- Servicio independiente en `apps/s3-gateway`, escuchando en `4002`, añadido a
  `build-all.sh` y `run-all.sh`; no usa AWS S3: usa exclusivamente el endpoint
  S3-compatible de Cloudflare R2 y sus credenciales gestionadas por Vault.
- Verifica solicitudes AWS Signature Version 4 y valida la API key mediante
  `KeyValidationService`, exigiendo `files:read`, `files:write` o
  `files:delete` según la operación. El secreto de firma es la API key
  entregada al cliente; la clave nunca se persiste en el gateway.
- Delega ownership, cuotas, creación de metadatos, confirmación y borrado
  lógico al `file-service`; las claves de objeto se fuerzan al namespace
  `s3/{user_id}/` y no aceptan traversal ni acceso cruzado entre usuarios.
- Implementa `GET`, `PUT`, `DELETE`, listado de bucket, `Range`/`206 Partial
  Content`, streaming de descarga, multipart (`uploads`, `uploadId`,
  `partNumber`) y confirmación con hash SHA-256 calculado en streaming.
- Las subidas simples y multipart están limitadas por tamaño configurable del
  servicio, usan backpressure y no cargan objetos completos en memoria durante
  descarga ni durante el cálculo de hash.

- Crear `file-service` independiente como bounded context de archivos, con
  PostgreSQL propio. No se implementará la lógica de almacenamiento dentro de un
  BFF.
- Mantener `customer-bff` como plano de control HTTP para el frontend: sesiones,
  permisos, cuotas, URLs firmadas y operaciones de metadatos.
- El `s3-gateway` ya implementa firmas AWS/SigV4, streaming, multipart upload y
  range requests; la autorización y ownership se delegan al `file-service`.
- Usar R2 como plano de datos; PostgreSQL solo almacenará metadatos y estados.
- Persistir metadatos: `file_id`, `user_id`, nombre, MIME, tamaño, hash, estado, versión y timestamps.
- Subir originales a R2 mediante URLs firmadas.
- Validar MIME real, extensión, tamaño y antivirus/escaneo antes de ingerir.
- Implementar estados: `pending`, `processing`, `ready`, `failed`, `deleted`.
- Aplicar cuotas atómicamente para evitar carreras de subida.
- Implementar carpetas, listado, renombrado, movimiento y borrado lógico.
- Exponer API privada para acceso compatible con S3, siempre con scopes y URLs firmadas.

### Fase 4 — Ingestión y embeddings

#### Estado de implementación (2026-07-25)

- `file-service` publica `file.ingest.requested` en `event_outbox` dentro de la misma transacción que confirma el archivo.
- Un `OutboxRelay` publica el evento en JetStream `EVENTS`; el consumidor durable `file-ingestion-worker` procesa hasta 16 mensajes concurrentes por lote, confirma con double-ACK y aplica backoff.
- Los fallos se reintentan y, al superar `DEFAULT_MAX_DELIVERIES`, se trasladan a `EVENTS_DLQ`; el estado SQL pasa a `failed` o `dead_letter` y se conserva el error.
- La migración crea `ingestion_jobs` y `embedding_chunks` con idempotencia por archivo/estrategia/modelo, conteo de tokens, costo en micros y estado operativo.
- Se extrae texto de PDF mediante parser dedicado, DOCX/XLSX mediante ZIP/XML y TXT/CSV mediante UTF-8. La estrategia `v1-token-800-overlap-120` está versionada y rechaza estrategias desconocidas.
- El cliente de embeddings usa un endpoint OpenAI-compatible configurado fuera del código; los vectores se escriben en Qdrant con IDs deterministas y payload `user_id`/`file_id`.
- La implementación inicial usa OpenRouter como proveedor OpenAI-compatible para todos los planes con `qwen/qwen3-embedding-8b`; las diferencias entre planes se aplican mediante cuotas, tamaño, concurrencia y acceso a RAG, no mediante un modelo distinto.
- Antes de descargar o procesar el objeto se vuelve a validar el plan y el tamaño máximo; planes sin embeddings o archivos fuera de cuota terminan como fallos trazables.
- Se emiten métricas de completados, fallos y DLQ, además de trazas OTEL del flujo existente.

Configuración operativa requerida (secretos en Vault; no se almacenan en `.env`): `EMBEDDING_API_URL`, `EMBEDDING_API_KEY`, `EMBEDDING_MODEL`, `EMBEDDING_COST_MICROS_PER_1K`, `INGESTION_STRATEGY_VERSION`, `QDRANT_URL`, `QDRANT_API_KEY` y `QDRANT_EMBEDDING_COLLECTION`. Para OpenRouter: `EMBEDDING_API_URL=https://openrouter.ai/api` y `EMBEDDING_MODEL=qwen/qwen3-embedding-8b`.

- Crear pipeline asíncrono con Outbox → NATS JetStream.
- Extraer texto de PDF, DOCX, XLSX y TXT.
- Fragmentar con estrategia versionada.
- Generar embeddings con el modelo configurado.
- Registrar costo, tokens y estado de cada operación.
- Enviar fallos a DLQ con reintentos y ACK.
- No permitir que un archivo exceda los límites de su plan.

### Fase 5 — Qdrant

#### Estado de implementación (2026-07-24)

- Starter, Pro y Pro Plus usan la colección compartida del entorno por defecto; la dedicada solo se habilita mediante la capacidad contractual explícita `dedicated_qdrant_enabled=true`.
- Cada punto almacena `user_id`, `file_id`, `document_id`, `chunk_id`, `chunk_index`, `content_hash`, modelo, versión del embedding y estrategia de fragmentación.
- Los índices de payload de Qdrant para `user_id`, `file_id` y `document_id` se crean automáticamente.
- El filtro de propiedad lo construye exclusivamente `file-service`; los contratos públicos no aceptan `user_id` ni filtros Qdrant suministrados por el cliente.
- Cada respuesta de Qdrant se valida nuevamente contra `user_id` y contra archivos activos en PostgreSQL antes de devolver contenido.
- `vector_usage` mantiene por usuario cantidad de vectores, bytes vectoriales y documentos, recalculados idempotentemente después de inserciones y borrados.
- El borrado lógico de un archivo publica `file.vectors.delete.requested` en la misma transacción. Un consumidor durable elimina los puntos y metadatos con ACK, reintentos y DLQ.
- Endpoints autenticados del `customer-bff`:
  - `POST /api/v1/vectors/search`.
  - `GET /api/v1/vectors/usage`.
  - `DELETE /api/v1/files/{id}/vectors`.
  - `POST /api/v1/files/{id}/vectors/reindex`.
- Las operaciones de borrado y reindexado verifican propiedad y plan en backend y quedan registradas en `file_audit_log`.

- Corregir la topología actual: colección compartida para Starter, Pro y Pro
  Plus estándar; colección dedicada solo para Pro Plus 75 USD cuando exista una
  capacidad contractual explícita.
- Guardar payload mínimo: `user_id`, `file_id`, `document_id`, `chunk_id`, `content_hash` y versión del embedding.
- Hacer obligatorio el filtro de `user_id` en cada consulta, inserción y borrado.
- Prohibir que el cliente envíe o sustituya el filtro de propietario.
- Medir vectores y bytes usados por usuario.
- Mantener `qdrant_mode=dedicated` como capacidad contractual de Pro Plus 75
  USD; no crear ni inferir un cuarto plan Enterprise.
- Implementar borrado coordinado de vectores al borrar un archivo.

### Fase 4A — Endurecimiento de Qwen3-Embedding-8B y OpenRouter

Objetivo: usar la capacidad real del modelo sin exceder su contexto ni registrar
costos incorrectos.

- Usar exclusivamente `qwen/qwen3-embedding-8b` desde backend; el frontend y las
  API keys públicas no pueden seleccionar ni sustituir el modelo.
- Incorporar el tokenizer oficial de Qwen y fijar su artefacto mediante SHA-256.
  El servicio debe usar una copia local cacheada y validada, no descargar un
  tokenizer mutable en cada arranque.
- Cambiar la estrategia nueva a `v2-qwen-token-800-overlap-120`. La estrategia
  `v1-token-800-overlap-120` se conserva solo para poder reconstruir datos
  históricos; no debe confundirse conteo de palabras con conteo de tokens.
- Limitar cada entrada a 32.768 tokens, incluidos tokens especiales. Una entrada
  fuera del límite se rechaza antes de llamar a OpenRouter.
- Mantener overlap de 120 sobre chunks de 800 tokens, aproximadamente 15 %.
- Agrupar entradas mediante dos límites simultáneos:
  `EMBEDDING_MAX_BATCH_INPUTS` y `EMBEDDING_MAX_BATCH_TOKENS`.
- Procesar batches con concurrencia limitada, backpressure y orden estable de
  resultados. Nunca lanzar un número de peticiones proporcional al tamaño del
  documento sin límite.
- Reintentar únicamente timeouts, `408`, `429` y `5xx`, respetando
  `Retry-After`, con exponential backoff y jitter. Los `4xx` permanentes no se
  reintentan.
- Enviar `encoding_format=float` y una dimensión configurada y validada. Para
  Qwen3-Embedding-8B la dimensión predeterminada será 4096; cualquier respuesta
  vacía, no finita, desordenada o con dimensión distinta falla cerrada.
- Usar la instrucción de retrieval de Qwen solo para la consulta:
  `Instruct: {tarea}\nQuery:{consulta}`. Los documentos se embeben sin permitir
  que el cliente inyecte una instrucción.
- Registrar tokens facturados devueltos por OpenRouter. El conteo local sirve
  para límites y validación, pero no reemplaza el uso facturado cuando el
  proveedor lo informa.
- Configurar `EMBEDDING_COST_MICROS_PER_1K=10`, equivalente a
  0,01 USD por un millón de tokens.
- Añadir métricas de latencia, reintentos, batches, tokens, dimensión, errores
  por categoría y costo, sin registrar textos, tokens de API ni documentos.

Variables operativas no secretas:

- `EMBEDDING_MAX_INPUT_TOKENS=32768`.
- `EMBEDDING_DIMENSIONS=4096`.
- `EMBEDDING_MAX_BATCH_INPUTS=16`.
- `EMBEDDING_MAX_BATCH_TOKENS=32768`.
- `EMBEDDING_MAX_CONCURRENT_REQUESTS=4`.
- `EMBEDDING_MAX_RETRIES=3`.
- `EMBEDDING_RETRY_BASE_MS=250`.
- `EMBEDDING_RETRY_MAX_MS=4000`.
- `EMBEDDING_REQUEST_TIMEOUT_SECONDS=120`.
- `EMBEDDING_QUERY_INSTRUCTION`.
- `EMBEDDING_TOKENIZER_PATH`.
- `EMBEDDING_TOKENIZER_URL` y `EMBEDDING_TOKENIZER_SHA256`, ambos fijados a una
  versión concreta del artefacto oficial.

La única variable secreta de este bloque es `EMBEDDING_API_KEY`, gestionada por
Vault.

#### Estado de implementación (2026-07-24)

- Completada en `file-service`: el modelo queda fijado en backend a
  `qwen/qwen3-embedding-8b` y el endpoint queda restringido a HTTPS de
  `openrouter.ai`; el frontend no puede sustituir ninguno de los dos valores.
- El tokenizer oficial está fijado al commit
  `4e423935c619ae4df87b646a3ce949610c66241c` y al SHA-256
  `83cdf8c3a34f68862319cb1810ee7b1e2c0a44e0864ae930194ddb76bb7feb8d`.
  Se cachea localmente, se verifica antes de cada uso y se descarga de forma
  atómica solo cuando no existe.
- `v2-qwen-token-800-overlap-120` fragmenta por IDs reales del tokenizer,
  conserva overlap 120/800 y cuenta los tokens reales de cada chunk. La
  estrategia v1 de palabras se conserva solo para reconstrucción histórica.
- Cada entrada se valida antes de llamar al proveedor contra el máximo de
  32.768 tokens. Las peticiones se agrupan por número de entradas y tokens,
  con concurrencia acotada y orden estable.
- El cliente valida índices, cantidad, finitud y dimensión exacta 4096 de cada
  vector. Qdrant también se inspecciona antes de escribir para impedir una
  colección con dimensión incompatible.
- Se implementaron reintentos únicamente para timeout/conexión, 408, 429 y
  5xx, con `Retry-After`, backoff exponencial y jitter. Los errores permanentes
  no se reintentan y las métricas no contienen contenido ni credenciales.
- Las consultas usan exclusivamente la instrucción backend de Qwen
  (`Instruct: ...\nQuery:...`); los documentos no aceptan instrucciones del
  cliente. `input_tokens` usa el uso facturado por OpenRouter cuando está
  disponible y el conteo local como fallback.
- Se configuró `EMBEDDING_COST_MICROS_PER_1K=10`, equivalente a USD 0,01 por
  millón de tokens, junto con los límites operativos en `.env.example`.
- Validación realizada: `cargo test -p file-service --bin file-service` (10
  tests OK, 2 integraciones DB y 1 descarga opcional del tokenizer ignoradas), `cargo clippy -p file-service
  --all-targets --all-features --no-deps -- -D warnings` y `cargo check
  -p file-service`.

### Fase 4B — Deduplicación canónica de embeddings

Objetivo: que `content_hash` sea una regla activa y no solamente metadato.

La deduplicación inicial será estrictamente por usuario. No se reutilizarán
vectores entre usuarios aunque el texto sea idéntico: una caché global introduce
canales laterales de igualdad de contenido, complica el derecho de borrado y
eleva el impacto de una autorización defectuosa.

#### Estado de implementación (2026-07-24)

- Migración `20260724000106` aplicada: crea `embedding_vectors`,
  `embedding_chunk_refs`, `embedding_vector_deletions` y métricas de ahorro en
  `ingestion_jobs`; conserva y migra los datos de `embedding_chunks`.
- Migración correctiva `20260724000107` normaliza nombres de colecciones
  canónicas sin modificar la migración ya aplicada.
- La clave única real es
  `(user_id, content_hash, embedding_model, embedding_version)`. No existe caché
  ni punto compartido entre usuarios.
- Cada hash faltante se reserva con una lease. `INSERT ... ON CONFLICT` y el
  estado `embedding` impiden que dos workers llamen simultáneamente a
  OpenRouter para la misma clave.
- Una lease fallida o vencida puede reclamarse; una lease activa de otro worker
  provoca reintento por JetStream sin duplicar el consumo.
- Los chunks repetidos dentro de un archivo y entre archivos del mismo usuario
  reutilizan el vector canónico y crean referencias independientes.
- Solo los cache misses se envían al proveedor. `ingestion_jobs` persiste
  `cache_hits`, `cache_misses`, `tokens_avoided` y
  `cost_avoided_micros`.
- Las métricas Prometheus registran hits, misses, tokens evitados, costo evitado,
  sincronizaciones pendientes, fallos de mantenimiento y vectores eliminados.
- Qdrant usa la colección de esquema `nexus_embeddings_v2`. Cada punto contiene
  únicamente identidad canónica y `user_id`; los archivos, documentos, chunks
  y posiciones se resuelven en PostgreSQL después de la búsqueda.
- El filtro Qdrant siempre incluye `user_id`. Las búsquedas por archivo añaden
  `has_id` usando IDs obtenidos desde PostgreSQL, nunca desde el cliente.
- El borrado de un archivo elimina sus referencias y recalcula el contador. El
  punto se encola para borrado físico solamente cuando `reference_count` llega
  a cero.
- La cola SQL de borrado permite reintentar una caída entre PostgreSQL y
  Qdrant. Un worker de mantenimiento también sincroniza el backfill pendiente y
  reintenta eliminaciones.
- Las actualizaciones de referencias y los borrados bloquean la fila del
  archivo, evitando carreras entre reindexado y eliminación.
- Pruebas reales con PostgreSQL verifican exclusión de workers, cache hit,
  separación entre usuarios y conservación del vector hasta borrar su última
  referencia.
- Prueba real con Qdrant verifica que `user_id + has_id` no permite borrar un
  punto perteneciente a otro usuario.

Modelo de datos:

- `embedding_vectors` representa el vector canónico y tendrá:
  `id`, `user_id`, `content_hash`, `embedding_model`, `embedding_version`,
  `vector_dimension`, `vector_bytes`, `collection_name`, `qdrant_point_id`,
  `status`, `lease_until`, `reference_count`, timestamps y último error.
- Restricción única:
  `(user_id, content_hash, embedding_model, embedding_version)`.
- `embedding_chunk_refs` representa cada aparición del contenido y tendrá:
  `user_id`, `file_id`, `document_id`, `chunk_id`, `chunk_index`,
  `strategy_version`, `embedding_vector_id` y timestamps.
- Un mismo vector canónico puede tener varias referencias dentro del mismo
  documento o en documentos diferentes del mismo usuario.
- El hash se calcula sobre el contenido normalizado exacto del chunk después del
  chunking, junto con dominio de versión. El modelo, dimensión y versión forman
  parte de la clave de deduplicación; cambiar cualquiera de ellos exige un
  vector nuevo.

Flujo concurrente:

1. Calcular hashes de todos los chunks.
2. Consultar en una sola operación cuáles existen para el mismo `user_id`,
   modelo y versión.
3. Reservar los faltantes mediante `INSERT ... ON CONFLICT` y una lease. Solo el
   propietario de la lease llama a OpenRouter.
4. Los workers competidores reutilizan el vector completado o reintentan una
   lease expirada; nunca generan simultáneamente el mismo embedding.
5. Insertar las referencias de chunks idempotentemente.
6. Publicar en Qdrant mediante un ID determinista derivado del vector canónico.
7. Marcar `completed` y actualizar `reference_count` dentro de transacciones
   breves; las llamadas remotas nunca mantienen una transacción SQL abierta.

Los cache hits no consumen OpenRouter. Deben registrar:

- `embedding_cache_hits_total`.
- `embedding_cache_misses_total`.
- `embedding_tokens_avoided_total`.
- `embedding_cost_avoided_micros_total`.
- Ratio de deduplicación por plan, modelo y estrategia, sin `user_id` en labels.

### Fase 4C — Reindexado incremental y eliminación referencial

#### Estado de implementación (2026-07-24): completada

- `commit_file_references` ejecuta un diff dentro de una transacción que toma
  bloqueo de la fila del archivo. La identidad de un chunk es su posición
  (`chunk_index`) junto con estrategia, modelo y versión; las referencias cuyo
  vector no cambió conservan su `id` y no se vuelven a insertar.
- Un chunk modificado actualiza únicamente el vínculo al nuevo vector. Los
  chunks nuevos crean una referencia y los que desaparecen se eliminan en la
  misma transacción. Las referencias de estrategias o modelos anteriores se
  consideran obsoletas y se retiran al activar el índice actual.
- Los vectores antiguos y nuevos se incluyen en el recálculo atómico de
  `reference_count`; un vector solo entra en `embedding_vector_deletions` cuando
  queda sin referencias. El mantenimiento existente reintenta el borrado físico
  de Qdrant sin perder el registro SQL ante una caída.
- `ingestion_jobs` registra `source_sha256`, `chunks_retained`, `chunks_added`,
  `chunks_removed`, `started_at` y `completed_at` mediante la migración
  `20260724000108_add_incremental_reindex_metadata.sql` y la corrección
  `20260724000109_fix_source_hash_check.sql`. Las métricas y logs
  incluyen el diff, el hash de fuente y el costo/tokenizado evitado, sin
  exponer contenido ni secretos.
- La operación es idempotente frente a redelivery: un trabajo completado se
  omite, un reindexado repetido conserva las referencias y la deduplicación
  canónica evita volver a llamar al proveedor para hashes existentes. El
  bloqueo por archivo serializa reindexado y borrado del mismo propietario.

Validación realizada: migración aplicada en `files_db`, `cargo check -p
file-service --all-targets` y prueba de integración PostgreSQL de borrado
referencial exitosa. No se levantaron microservicios ni se realizaron llamadas
pagadas al proveedor de embeddings.

- Comparar el conjunto anterior y nuevo de hashes al actualizar o reindexar un
  archivo.
- Mantener las referencias que no cambiaron.
- Crear referencias para chunks repetidos ya existentes.
- Enviar a OpenRouter solamente hashes nuevos.
- Eliminar referencias de chunks que desaparecieron.
- Decrementar referencias de vectores antiguos atómicamente.
- Publicar `file.vector.orphaned` mediante Outbox cuando el contador llegue a
  cero; el consumidor elimina el punto canónico en Qdrant y después el registro
  SQL.
- Un borrado de archivo elimina únicamente sus referencias. Nunca elimina un
  vector todavía utilizado por otro archivo del mismo usuario.
- El proceso debe ser idempotente frente a redelivery de JetStream, caída entre
  Qdrant/SQL y reindexados simultáneos.
- Mantener estados `reserved`, `embedding`, `ready`, `failed` y `deleting`, con
  recuperación de leases abandonadas.

### Fase 5A — Topología Qdrant y resolución de referencias

#### Estado de implementación (2026-07-24): completada

- Todos los planes del catálogo (`starter`, `pro` y `pro-plus`, incluidos los
  contratos Pro Plus de 75 USD) quedan en colección compartida por defecto.
  `qdrant_mode` ya no concede una colección dedicada por sí solo.
- La modalidad dedicada solo se resuelve cuando el plan tiene la capacidad
  contractual explícita `features.dedicated_qdrant_enabled=true`; la migración
  `20260724000301_enforce_qdrant_topology_capability.sql` y
  `20260724000302_rename_qdrant_capability_constraint.sql` normalizan el
  estado legado y dejan Pro Plus compartido por defecto.
- Las colecciones se validan como nombres seguros con sufijo `_v2`; la colección
  dedicada se deriva exclusivamente del UUID autenticado (`<base>_<user_uuid>`)
  y nunca de una entrada del cliente.
- Cada punto canónico contiene `user_id`, `embedding_vector_id`,
  `content_hash`, `embedding_model`, `embedding_version` y `strategy_family`.
  La migración `20260724000110_add_canonical_strategy_family.sql` añade el
  campo persistido e índices de payload; `20260724000111_resync_canonical_payload_schema.sql`
  fuerza la reescritura de puntos existentes para que no quede ningún payload
  antiguo activo.
- Upsert, búsqueda y borrado construyen siempre filtros con `user_id` desde la
  identidad autenticada. Las respuestas de Qdrant se validan por payload y se
  resuelven nuevamente contra referencias autorizadas en PostgreSQL; solo se
  devuelven vectores `ready`, sincronizados y pertenecientes al usuario.
- La cuota de vectores usa `embedding_vectors` canónicos; documentos y chunks
  se contabilizan mediante `embedding_chunk_refs` y `vector_usage`. El borrado
  físico solo ocurre cuando el último vínculo desaparece.

Validación realizada: migraciones 5A aplicadas en `subscription_db` y
`files_db`, pruebas de aislamiento/filtros y pruebas PostgreSQL de deduplicación,
reindexado y borrado referencial exitosas; `cargo check --workspace`, clippy del
`file-service` y validación de Compose correctos.

Topología objetivo:

- Starter, Pro y Pro Plus de 20 USD usan una colección compartida por entorno
  con filtro obligatorio `user_id`.
- Pro Plus de 75 USD también usa la compartida por defecto.
- Una colección dedicada se habilita únicamente mediante una capacidad
  contractual explícita, por ejemplo `dedicated_qdrant_enabled=true`. No se
  infiere solamente del slug del plan.
- No existe un cuarto plan Enterprise; Pro Plus 75 USD es la modalidad
  empresarial y puede recibir ese add-on.

Para deduplicar también el almacenamiento Qdrant, cada punto pasa a representar
un vector canónico. Su payload mínimo será:

- `user_id`.
- `embedding_vector_id`.
- `content_hash`.
- `embedding_model`.
- `embedding_version`.
- `strategy_family`.

`file_id`, `document_id`, `chunk_id` y `chunk_index` permanecen en
`embedding_chunk_refs`. Después de buscar en Qdrant, `file-service` resuelve las
referencias autorizadas en PostgreSQL y devuelve resultados por archivo. Esto
evita duplicar un vector por cada aparición del mismo texto sin perder
proveniencia ni borrado correcto.

Reglas obligatorias:

- El cliente nunca envía `user_id`, nombre de colección, filtros Qdrant ni
  `qdrant_point_id`.
- Búsqueda, upsert, scroll, conteo y borrado construyen `user_id` desde la
  identidad autenticada.
- Toda respuesta de Qdrant se vuelve a validar contra PostgreSQL antes de
  devolver contenido.
- Las colecciones nuevas usan sufijo de esquema, por ejemplo
  `nexus_embeddings_v2`, para no mezclar payloads antiguos y canónicos.
- La cuota vectorial se contabiliza por vectores canónicos; la cuota de
  documentos/chunks se contabiliza por referencias.

### Fase 5B — Límites de ingestión por plan

**Estado: completada.** La migración `20260724000303` añade los seis límites al
contrato de planes, PostgreSQL y gRPC; `file-service` reserva capacidad antes de
publicar Outbox y el consumidor mantiene la reserva durante los reintentos.

Además de `max_file_size_bytes`, `max_files` y almacenamiento total, añadir:

- `max_ingestion_files_per_hour`.
- `max_ingestion_files_per_day`.
- `max_ingestion_bytes_per_day`.
- `max_ingestion_tokens_per_month`.
- `max_concurrent_ingestions`.
- `max_pending_ingestion_jobs`.

Aplicación:

- Redis aplica ventanas de hora/día/mes, reservas pendientes y concurrencia con
  scripts Lua atómicos; ante una pérdida de caché se reconstruye desde
  `ingestion_quota_usage` en PostgreSQL.
- PostgreSQL conserva el consumo autoritativo y permite reconstruir Redis.
- La reserva se realiza antes de publicar el evento; si no hay capacidad, no se
  descarga R2 ni se llama a OpenRouter.
- JetStream limita el consumidor de ingestión a 32 ACK pendientes y el worker
  extrae un mensaje por lote, aplicando backpressure antes de leer R2.
- Los límites quedan administrables desde Admin BFF. `file-service` resuelve la
  suscripción en cada reserva (sin una caché local obsoleta), por lo que un
  cambio de plan se aplica inmediatamente.
- Los cache hits de embeddings no consumen presupuesto de tokens facturados,
  pero sí respetan concurrencia y límites de archivos para impedir abuso.

### Fase 8A — Pruebas adversariales y observabilidad

Estado: **completada**. El binario `file-service` expone métricas Prometheus en
`FILE_SERVICE_METRICS_PORT` (9101 por defecto), Prometheus las scrapea y Grafana
provisiona el dashboard `Nexus File Service · Ingestion & Qdrant`. Las alertas
de `docker/observability/prometheus-rules.yml` cubren fallos, DLQ, `429`,
latencia, backlog, backpressure, leases, huérfanos y cruces de usuario. El
exporter `natsio/prometheus-nats-exporter` expone profundidad y estado de
JetStream para el mismo dashboard.

La suite adversarial se mantiene como pruebas unitarias deterministas y pruebas
de integración marcadas `#[ignore]` que requieren las bases locales. La matriz
de ejecución obligatoria es:

| Escenario | Garantía verificada |
|---|---|
| A busca/lee/reindexa/borra un archivo o chunk de B | `user_id` se deriva del límite de autorización y las consultas SQL devuelven `not_found`; no se genera evento para B. |
| Filtro, colección, point ID o `user_id` manipulados | El servicio construye filtros y colecciones desde identidad/configuración; se ignoran o rechazan valores del cliente. |
| Qdrant responde un punto de B a A | La validación de payload descarta el punto, incrementa `qdrant_cross_user_attempts_total` y emite alerta. |
| Dos workers reservan el mismo hash | La restricción única y la reserva transaccional dejan un único propietario de embedding. |
| Hash repetido en archivos/referencias | Existe un vector canónico y múltiples `embedding_chunk_refs`; solo la última referencia encola el borrado. |
| Reindexado parcial | El hash permanece en caché y solo se llama al proveedor para hashes nuevos. |
| Fallo/timeout/429/5xx, redelivery, lease y DLQ | JetStream redelivera con ACK explícito; el lease se pausa/libera y el mensaje termina en `EVENTS_DLQ` tras el límite. |
| Caída entre Qdrant y PostgreSQL | La cola de sincronización/borrado es idempotente y la tarea de mantenimiento reconstruye el estado. |
| Chunking v2 `800/120` | No produce chunks vacíos, no entra en bucle y mantiene el solapamiento acotado. |
| Límites por plan y subida masiva | Redis aplica reserva atómica, PostgreSQL conserva el uso autoritativo y JetStream aplica backpressure. |

Validación local (sin levantar microservicios):

```text
SQLX_OFFLINE=true cargo test -p file-service --bin file-service
SQLX_OFFLINE=true cargo test -p file-service --bin file-service -- --ignored
docker compose config --quiet
```

Pruebas mínimas obligatorias:

- Usuario A intenta buscar, leer, reindexar y borrar `file_id`/`chunk_id` de B.
- Usuario A intenta enviar `user_id=B`, un filtro Qdrant, colección o point ID
  manipulados; el contrato debe ignorarlos o rechazarlos.
- Qdrant devuelve deliberadamente un punto de B ante una consulta de A; la
  segunda validación debe descartarlo y emitir alerta.
- Dos workers intentan embeber el mismo hash simultáneamente; solo uno llama al
  proveedor.
- El mismo chunk aparece varias veces y en varios archivos; existe un vector y
  varias referencias.
- Borrar una referencia no elimina el vector compartido; borrar la última sí.
- Reindexar un documento con un cambio pequeño genera embeddings únicamente para
  los hashes nuevos.
- El mismo texto de usuarios distintos nunca comparte fila SQL ni punto
  Qdrant.
- Se prueban redelivery, lease expirada, timeout, `429`, `5xx`, DLQ y
  recuperación después de una caída entre Qdrant y PostgreSQL.
- Se verifica que overlap 120/800 no cree chunks vacíos, bucles ni una
  amplificación inesperada de tokens.
- Pruebas de carga para límites por plan, subida masiva y backpressure.

Dashboards y alertas:

- Tasa de cache hit/miss y ahorro estimado.
- Tokens/costo por plan y resultado.
- Latencia y errores de OpenRouter.
- Batches, reintentos, `429`, DLQ y leases expiradas.
- Profundidad de JetStream y trabajos pendientes.
- Vectores canónicos, referencias, bytes y huérfanos.
- Intentos de acceso cruzado y descartes defensivos.

### Cierre Fase 8 — Observabilidad y seguridad

Estado: **completada**.

- `crates/common::observability::hash_user_id` genera etiquetas estables de 24
  caracteres a partir de una sal de despliegue (`OBSERVABILITY_USER_HASH_SALT`,
  generada y administrada automáticamente por Vault bootstrap, con el secreto
  de servicio como respaldo). Ninguna métrica nueva publica el UUID del usuario.
- `file-service` crea spans OTEL para el consumidor JetStream, el lote de
  OpenRouter, la sincronización Qdrant y la consulta RAG. El contexto W3C se
  conserva desde HTTP/gRPC hasta NATS.
- `file_audit_log` registra de forma durable listado de carpetas/archivos,
  emisión de URLs de subida/descarga y consultas vectoriales, además de las
  escrituras, cambios y borrados existentes. Los metadatos no contienen
  contenido de documentos ni PII innecesaria.
- Prometheus incorpora alertas de rechazos de cuota, consumo anómalo
  pseudonimizado y detención de la auditoría; Grafana muestra operaciones de
  auditoría, cuotas y capacidad por usuario hasheado.
- `scripts/verify_fase8.sh` produce el reporte inmutable y el log en
  `artifacts/verify/fase8/`. Las pruebas de integración reales se habilitan
  con `FILE_SERVICE_DATABASE_URL` y `REDIS_URL`; no se usan mocks.

### Fase 9A — Migración, despliegue y rollback

Estado: **completada**.

- `20260724000113_add_v2_rollout_control.sql` crea el registro auditable de
  ejecuciones y reportes por usuario sin modificar ni borrar datos antiguos.
- `20260724000600_finalize_qdrant_rollout_topology.sql` corrige instalaciones
  existentes: ningún plan obtiene Qdrant dedicado por slug; solo una capacidad
  contractual explícita puede activarlo.
- `FILE_SERVICE_DUAL_WRITE_V1=true` mantiene la proyección legacy durante la
  ventana de estabilidad (PostgreSQL y la colección legacy de Qdrant).
  `FILE_SERVICE_VECTOR_READ_MODE` acepta `v2` (por
  defecto), `dual` (v2 servido y v1 comparado) o `v1` (rollback controlado).
  `QDRANT_EMBEDDING_COLLECTION_V1` identifica la colección legacy y nunca se
  acepta desde una petición del cliente.
- `scripts/file-service-v2-rollout.sh backfill` copia por usuario/hash/modelo/
  versión sin llamadas al proveedor. `compare` genera conteos, hashes y
  comprobaciones de aislamiento en PostgreSQL.
- `scripts/file-service-v1-cleanup.sh` exige backup no vacío, confirmación
  explícita y reportes sin discrepancias antes de borrar la tabla legacy.

Runbook recomendado:

Los scripts de backfill/compare/cleanup requieren `psql`; cleanup también
requiere `pg_dump` para producir el backup verificable.

```text
QDRANT_URL=http://127.0.0.1:6333 QDRANT_API_KEY=... ./scripts/qdrant-v2-bootstrap.sh
./scripts/file-service-v2-rollout.sh backfill
FILE_SERVICE_DUAL_WRITE_V1=true FILE_SERVICE_VECTOR_READ_MODE=dual ./run-all.sh
./scripts/file-service-v2-rollout.sh compare
# Tras la ventana de estabilidad y backup verificado:
FILE_SERVICE_VECTOR_READ_MODE=v2 ./run-all.sh
```

1. Crear tablas y columnas nuevas mediante migraciones aditivas.
2. Corregir el seed y la resolución de planes para que Pro Plus no implique
   Qdrant dedicado sin el add-on explícito.
3. Crear `nexus_embeddings_v2` e índices de payload.
4. Activar escritura dual controlada por feature flag.
5. Backfill por `user_id`, modelo, versión y hash, sin compartir datos entre
   usuarios.
6. Comparar resultados, conteos y aislamiento entre esquema antiguo y nuevo.
7. Activar lectura v2 de forma gradual y observable.
8. Detener escritura antigua después del período de estabilidad.
9. Conservar rollback a lectura v1 hasta verificar backups y reconstrucción.
10. Eliminar datos antiguos únicamente mediante una tarea auditada y
    recuperable.

Criterios de salida:

- Cero llamadas duplicadas a OpenRouter bajo carreras para una misma clave de
  deduplicación.
- Cero resultados, mutaciones o puntos compartidos entre usuarios.
- Costo calculado coincide con el usage del proveedor y su tarifa configurada.
- Reindexar un archivo sin cambios produce cero embeddings y cero nuevos
  vectores.
- Reindexar un cambio parcial procesa solamente los chunks nuevos.
- Los límites de ingestión se aplican antes de consumir R2, CPU o OpenRouter.
- Starter/Pro/Pro Plus estándar permanecen en colección compartida; solamente
  cuentas Pro Plus 75 USD autorizadas usan colección dedicada.

### Fase 6 — API keys y autorización

Estado: completada y validada con `cargo check` y Clippy estricto.

- Scopes separados y validados en backend: `inference:read`, `inference:write`,
  `files:read`, `files:write` y `files:delete`.
- `max_api_keys` se resuelve desde la suscripción activa; `0` representa sin
  límite numérico y no elimina los rate limits.
- La creación y la rotación tienen buckets de rate limit independientes.
- La rotación valida plan y capacidad antes de revocar la key anterior, para no
  dejar una rotación a medias si falla el servicio de suscripciones.
- Actualizar scopes, metadatos, revocar una key o cambiar la suscripción genera
  `api_key.authorization_changed` y avanza el epoch de autorización en Redis.
- Key-service y API Gateway invalidan sus cachés local y distribuido; no se
  mantiene una ventana de autorización obsoleta tras un cambio.
- R2/S3 exige `files:read`, `files:write` o `files:delete` según la operación;
  los modelos exigen `inference:write`.
- `user_id`, filtros Qdrant, colecciones y point IDs se derivan de la identidad
  autenticada; nunca se aceptan desde la API key o el frontend.
- Migración aditiva `20260724000200_harden_api_key_scopes.sql` aplicada con
  restricciones de scopes no vacíos y valores permitidos.

### Fase 7 — Frontends

Estado: **completada**.

Crear en el dashboard de cliente:

- `/storage` o `/files`.
- Explorador de archivos y carpetas.
- Subida con progreso.
- Estado de ingestión.
- Borrado, renombrado y descarga segura.
- Uso de almacenamiento y cuota restante.
- Configuración de API keys y scopes.
- Estado de embeddings/RAG.
- Suscripción, método de pago, cancelación y consumo.

Implementación cliente:

- `client-platform-dashboard/app/(dashboard)/files/page.tsx` usa únicamente
  contratos reales de `customer-bff`; calcula SHA-256 en navegador, solicita
  una URL firmada, sube directo a R2 con `XMLHttpRequest` y reporta progreso,
  confirma el objeto y refresca el estado persistido por `file-service`.
- Los controles de renombrado, borrado, descarga y reindexado llaman al BFF;
  ninguna identidad, colección Qdrant o filtro de propiedad se acepta desde el
  navegador.
- La búsqueda RAG muestra únicamente resultados ya filtrados y validados por
  `user_id` en backend. Los estados `pending`, `processing`, `ready` y `failed`
  se muestran sin inferir éxito de una subida HTTP.
- `/api-keys` permite crear, rotar y revocar claves con scopes separados y
  muestra los scopes autorizados devueltos por el backend.
- Las páginas existentes de suscripción, métodos PayPal, cancelación, planes,
  wallet e invoices cubren el ciclo de cobro y consumo.

En el Admin Dashboard:

- Capacidad usada por usuario.
- Documentos y colecciones.
- Estado de ingestión y DLQ.
- Consumo de embeddings.
- Cambios de plan y acciones de bloqueo auditadas.

Implementación administrativa:

- `admin-plataform-dashboard/app/(dashboard)/storage/page.tsx` permite buscar
  un usuario y consultar su snapshot de archivos, carpetas, vectores canónicos,
  bytes, colección, estados de ingestión y DLQ.
- `admin-bff` expone
  `GET /api/v1/admin/storage/users/{user_id}`. El BFF obtiene archivos,
  carpetas y uso vectorial por gRPC, consulta el estado de `EVENTS_DLQ` por
  JetStream y valida que cada respuesta pertenezca al `user_id` solicitado
  antes de serializarla.
- La ruta de almacenamiento está protegida por la sesión del dashboard. Las
  acciones de bloqueo, activación, borrado, cambio de plan y ajustes
  administrativos existentes siguen registrando eventos `tracing` con actor y
  objetivo.

Variables nuevas del contrato administrativo:

- `FILE_SERVICE_GRPC_ENDPOINT` en `.env` y `.env.example` para resolver el
  servicio de archivos con mTLS.
- `admin-bff/.env.example` documenta el mismo endpoint para despliegues fuera
  de `run-all.sh`.

### Fase 8 — Observabilidad y seguridad

- Trazas completas desde subida hasta consulta RAG.
- Métricas por `user_id` hasheado, plan y resultado; nunca exponer PII innecesaria.
- Alertas de cuota, errores de ingestión, DLQ y uso anómalo.
- Auditoría de lectura, escritura, descarga, borrado y consultas vectoriales.
- Pruebas de aislamiento intentando acceder a objetos de otro usuario.
- Pruebas de carrera para cuotas y subidas concurrentes.

### Fase 8B — Configuración administrable en caliente

**Estado:** planificada. Esta fase convierte la configuración de negocio que
hoy requiere editar `.env` y reiniciar procesos en configuración versionada en
PostgreSQL, administrable desde `admin-bff`, con propagación a los servicios sin
reinicio. No convierte secretos ni parámetros de red en datos de negocio.

#### 8B.1 — Contrato de configuración y persistencia

- Crear una configuración autoritativa por clave, alcance y versión:
  `runtime_config`, `plan_limits`, `plan_capabilities`, `pricing_config` y
  `config_change_log`.
- Cada valor debe tener tipo, unidad, versión, `effective_at`, actor
  administrativo, motivo, estado (`draft`, `active`, `retired`) y checksum.
- Usar migraciones aditivas e idempotentes; nunca sobrescribir un valor activo
  sin conservar la versión anterior.
- Validar rangos, dependencias y compatibilidad antes de publicar una versión.
- Aplicar optimistic concurrency (`expected_version`) para impedir que dos
  administradores pierdan cambios simultáneos.

#### 8B.2 — Límites por plan

Administrar desde el Admin Dashboard, con efecto inmediato tras invalidar caché:

- `max_storage_bytes`, `max_file_size_bytes`, `max_files`.
- `max_ingestion_files_per_hour` y `max_ingestion_files_per_day`.
- `max_ingestion_bytes_per_day` y `max_ingestion_tokens_per_month`.
- `max_concurrent_ingestions` y `max_pending_ingestion_jobs`.
- `max_api_keys`, límites de creación/rotación y rate limits por plan.
- Retención de archivos, vectores y métricas cuando la política del producto
  lo permita.

La reserva de cuota debe resolver siempre la versión activa desde PostgreSQL o
su caché validada; el frontend nunca puede enviar límites efectivos.

#### 8B.3 — Capacidades y feature flags comerciales

Administrar por plan y, cuando proceda, por usuario autorizado:

- Embeddings, RAG, búsqueda vectorial y reindexado.
- Modelos permitidos, streaming, respuestas largas y sobreconsumo.
- API compatible con S3 y scopes `files:read`, `files:write` y `files:delete`.
- `dedicated_qdrant_enabled` únicamente como capacidad contractual explícita;
  nunca inferirla por el slug o el precio del plan.
- Modo de mantenimiento o desactivación temporal de una capacidad, con fecha
  de inicio/fin y mensaje operativo.

Cada cambio debe publicar un evento de configuración versionado para invalidar
las decisiones cacheadas en `subscription-service`, `key-services`,
`file-service`, `customer-bff` y `admin-bff`.

#### 8B.4 — Precios, facturación y políticas de cobro

Mover a configuración versionada y auditada:

- Precio de cada plan y modalidad de prueba.
- Precio de tokens de entrada/salida y `EMBEDDING_COST_MICROS_PER_1K`.
- Impuestos, moneda comercial, umbral de saldo y multiplicadores de margen.
- TTL de reservas, período de gracia, reintentos de dunning y política de
  bloqueo.

Los precios deben incluir moneda, fecha de vigencia, redondeo y versión. Un
cambio no altera facturas históricas ni operaciones ya confirmadas; solo afecta
nuevas reservas o ciclos posteriores según `effective_at`.

#### 8B.5 — Modelos, embeddings y estrategia RAG

- Activar/desactivar modelos del catálogo y asociarlos a planes sin reinicio.
- Seleccionar la versión activa de `qwen/qwen3-embedding-8b` únicamente desde
  backend, manteniendo el proveedor OpenRouter y su clave en Vault.
- Versionar `INGESTION_STRATEGY_VERSION`, tokenizer, dimensiones, chunk size y
  overlap; un cambio de dimensión exige colección Qdrant nueva y migración
  controlada.
- No permitir que un administrador cambie silenciosamente el modelo de jobs ya
  iniciados; cada job conserva proveedor, modelo, versión y tarifa resueltos.

#### 8B.6 — Almacenamiento, R2 y Qdrant

- Administrar cuotas y políticas de retención desde `file-service`.
- Administrar límites de multipart, rangos, concurrencia y tamaño por plan.
- Administrar la política de colección compartida/dedicada sin permitir que el
  cliente elija colección o `user_id`.
- Mantener R2 endpoint, bucket, Access Key, Secret Key y Qdrant API key en
  Vault; el Admin Dashboard solo modifica políticas, no credenciales.
- Los cambios de cuota deben ser atómicos y no pueden crear saldo negativo de
  almacenamiento ni vectores huérfanos.

#### 8B.7 — Operación y observabilidad

- Administrar umbrales de alertas de cuota, errores, DLQ, latencia, costo y uso
  anómalo.
- Permitir un cambio controlado de nivel de log si el servicio soporta recarga;
  no alterar permanentemente `tracing` sin registrar expiración.
- Administrar límites de backpressure y ventanas de reintento por plan dentro
  de rangos seguros definidos por código.
- Mostrar versión activa, actor, fecha, motivo, impacto y rollback disponible.

#### 8B.8 — Autorización, auditoría, caché y rollback

- Solo roles administrativos explícitos pueden leer o mutar configuración; las
  claves secretas nunca se devuelven en respuestas.
- Registrar antes/después, actor, IP, request ID, versión y motivo en auditoría
  firmada.
- Incorporar un subject versionado `configuration.changed` mediante Outbox →
  JetStream y actualizar Redis con invalidación por versión; el TTL no puede
  ser el único mecanismo. Este evento todavía no forma parte de los 14 subjects
  actuales hasta completar esta fase.
- Mantener al menos la última versión estable y permitir rollback idempotente
  con `expected_version`.
- Si Redis o JetStream no están disponibles, rechazar la activación o conservar
  la versión anterior; nunca aplicar parcialmente una configuración.

#### 8B.9 — Configuración que permanece fuera del Admin Dashboard

No se moverán a PostgreSQL ni se expondrán por Admin BFF:

- Contraseñas, API keys, tokens, JWT secrets, peppers y claves de auditoría.
- Credenciales PayPal, R2, Qdrant, OpenRouter, Resend y AWS.
- URLs y credenciales de PostgreSQL, Redis, NATS, Vault y Qdrant.
- Puertos, bind addresses, CORS de infraestructura y endpoints gRPC.
- `TLS_MODE`, proveedores/certificados mTLS, AppRole, PKI y configuración de
  Vault.

#### 8B.10 — Endpoints administrativos y criterios de salida

El Admin BFF deberá exponer, protegido por sesión administrativa y auditoría:

- `GET /api/v1/admin/config` y `GET /api/v1/admin/config/history`.
- `PUT /api/v1/admin/config/{key}` con `expected_version` y `effective_at`.
- `POST /api/v1/admin/config/{key}/rollback`.
- Endpoints de límites/capacidades/precios por plan con validación de contrato.

La subfase se cerrará solamente cuando:

- Un cambio de límite o capacidad se propague sin reiniciar servicios.
- La reserva de cuota use la nueva versión y Redis se invalide correctamente.
- Un cambio de precio no modifique facturas históricas.
- Un cambio de modelo no afecte jobs ya iniciados ni mezcle dimensiones Qdrant.
- Dos administradores concurrentes no pierdan cambios.
- Rollback y recuperación tras caída de Redis/NATS sean idempotentes.
- Existan pruebas de autorización, auditoría, aislamiento y carga, más los
  tres artefactos del gate en `artifacts/verify/fase8b/`.

### Fase 9 — Rendimiento y producción

- Pruebas de carga de subida, ingestión y consultas.
- Límites de concurrencia por plan.
- Backpressure en NATS y workers.
- Reintentos idempotentes.
- Backups y restauración de metadatos y R2.
- Plan de recuperación de Qdrant y reconstrucción desde documentos originales.
- Smoke tests de pagos, cancelación, dunning y bloqueo.

#### Implementación de Fase 9 y validación pendiente

- `scripts/phase9_load_test.sh` ejecuta carga HTTP concurrente contra endpoints reales y calcula p50, p95 y p99 sin mocks; puede probar subida, ingestión y RAG configurando la URL, método, cuerpo y autorización del endpoint.
- `FILE_SERVICE_INGESTION_MAX_ACK_PENDING`, `FILE_SERVICE_INGESTION_BATCH_SIZE`, `FILE_SERVICE_VECTOR_DELETE_MAX_ACK_PENDING` y `FILE_SERVICE_VECTOR_DELETE_BATCH_SIZE` controlan el backpressure de JetStream y workers. Los valores se acotan y se publican como gauges.
- La reserva atómica de cuotas, la clave única de `ingestion_jobs`, la deduplicación canónica y el ACK/DLQ existente garantizan reintentos idempotentes y evitan llamadas duplicadas al proveedor.
- `scripts/backup_phase9.sh` realiza dumps custom de PostgreSQL, manifiesto de archivos, copia con checksum de R2 y snapshots de colecciones Qdrant. `scripts/restore_phase9.sh` exige `RESTORE_CONFIRM=RESTORE` y valida `SHA256SUMS` antes de restaurar.
- `scripts/qdrant_recovery_phase9.sh` genera un manifiesto de todos los documentos originales listos y, en modo explícito `execute`, despacha reindexaciones mediante el endpoint autenticado del file-service. `RECOVERY_USER_ID` es obligatorio en ejecución para procesar un propietario por vez y evitar cualquier cruce de identidad. La reconstrucción se hace desde R2 y PostgreSQL, no desde vectores incompletos.
- `apps/payment-services/scripts/paypal_sandbox_smoke.sh` valida OAuth, creación idempotente de orden y captura opcional; `scripts/payment_lifecycle_smoke_phase9.sh` valida estados, cancelación, reintentos y dunning en PostgreSQL.
- El gate `scripts/verify_fase9.sh` produce reporte JSON inmutable, `latest.json` y log en `artifacts/verify/fase9/`. Las operaciones destructivas y las cargas contra servicios vivos son opt-in mediante variables explícitas.

## 7. Criterios de aceptación

- Un usuario nunca puede consultar ni descargar datos de otro `user_id`.
- Una cuota no puede superarse por condiciones de carrera.
- Un archivo de tamaño inválido se rechaza antes de consumir recursos de embeddings.
- Un fallo de ingestión se reintenta y termina en DLQ con trazabilidad.
- Cancelar una suscripción bloquea las capacidades según la política definida.
- Un pago fallido activa dunning y no elimina datos del usuario.
- Todas las acciones administrativas quedan auditadas.
- Los límites del plan se aplican desde backend y no pueden alterarse desde el frontend.

## 9. Integración y validación futura

La implementación de código y lógica de las fases está completada según el
estado indicado en este documento. Antes de declarar el sistema listo para
producción al 100%, ejecutar en un entorno de staging/preproducción con
dependencias reales:

- Interoperabilidad de `s3-gateway` con AWS CLI, rclone, Cyberduck y SDKs S3:
  SigV4, subida simple, descarga, rangos, multipart, listado y borrado.
- Gate de Fase 5 con PostgreSQL, Redis y Qdrant reales: aislamiento por
  `user_id`, deduplicación, cuotas, conteos, bytes y borrado coordinado.
- Pruebas de carga de subida, ingestión y RAG con p50/p95/p99, límites por plan
  y backpressure.
- Backup y restauración en un entorno aislado, incluyendo metadatos, R2 y
  reconstrucción de Qdrant desde los documentos originales.
- Smoke tests de PayPal Sandbox, cancelación, dunning, período de gracia y
  bloqueo.
- Repetir `cargo test --workspace` cuando esté disponible la resolución DNS
  requerida por `common::ssrf::tests::allows_public_https`; el fallo actual es
  ambiental y no está relacionado con estas funcionalidades.

Estas actividades son de integración y certificación operativa; no representan
funcionalidad pendiente de implementar en los servicios.
