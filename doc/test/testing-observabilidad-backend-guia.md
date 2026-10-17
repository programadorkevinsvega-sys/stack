# Guía de Testing y Observabilidad para Sistemas Robustos

Documento de referencia general — aplicable a cualquier proyecto backend (no específico de un solo sistema). Pensado para usarse como checklist al iniciar un proyecto nuevo y como referencia continua durante su crecimiento.

---

## Cómo leer este documento

Cada punto incluye:
- **Qué es** — definición corta y concreta.
- **Para qué sirve** — el problema real que resuelve.
- **Por qué importa** — la consecuencia de no tenerlo.
- **Herramientas** — opciones concretas (ecosistema Rust cuando aplica, más alternativas generales).
- **Etapa de implementación** — cuándo conviene introducirlo, para no tener que rediseñar nada después.

Al final hay una **tabla de priorización global** ordenando todo por etapa, para usar como roadmap.

---

# PARTE 1 — TESTING

## 1.1 Pruebas unitarias

**Qué es:** pruebas de una función o unidad de código aislada, sin dependencias externas (sin base de datos, sin red, sin otros servicios).

**Para qué sirve:** verificar que la lógica interna es correcta en aislamiento — cálculos, validaciones, transformaciones de datos.

**Por qué importa:** es la base más barata y rápida de detectar errores; sin esto, cualquier cambio pequeño puede romper lógica crítica sin que nadie lo note hasta producción.

**Herramientas:** `#[test]` nativo de Rust, `cargo test`.

**Etapa de implementación:** **Día 0.** No negociable, se escribe junto con el código de producción desde la primera línea del proyecto.

---

## 1.2 Pruebas de integración

**Qué es:** pruebas que verifican cómo interactúan varios componentes juntos — el servicio hablando de verdad con base de datos, con una cola de mensajes, con otro microservicio por gRPC/HTTP real.

**Para qué sirve:** detectar problemas que las pruebas unitarias no ven porque ahí todo está simulado — migraciones que no crean el índice esperado, filtros de aislamiento que no funcionan, deserialización que falla contra el formato real de un proveedor externo.

**Por qué importa:** es donde se detecta la mayoría de bugs de "funcionaba en mi cabeza pero no en la realidad". Sin esto, los errores de integración solo aparecen en producción.

**Herramientas:** `cargo test` + `testcontainers-rs` (levanta bases de datos/colas reales en contenedores efímeros solo para la prueba).

**Etapa de implementación:** **Día 0 a temprano en el desarrollo.** Se introduce apenas existe el primer componente externo real (base de datos, cola, servicio dependiente). No esperar a tener "todo el sistema listo" para empezar.

---

## 1.3 Property-based testing (pruebas basadas en propiedades)

**Qué es:** en vez de probar casos puntuales, se generan miles de entradas aleatorias y se verifica que una propiedad/invariante siempre se cumpla.

**Para qué sirve:** encontrar el caso límite raro que nadie pensaría en escribir manualmente (ej. "una cuota reservada nunca debe superar el límite del plan", "un chunk de texto nunca debe quedar vacío").

**Por qué importa:** los tests manuales solo cubren los casos que el autor imaginó; los bugs reales suelen vivir en los casos que nadie imaginó.

**Herramientas:** `proptest`, `quickcheck`.

**Etapa de implementación:** **Desarrollo temprano**, apenas exista lógica de negocio con invariantes claras (cálculos de costo, validaciones de cuota, chunking, parsers).

---

## 1.4 Idempotency / replay testing

**Qué es:** pruebas que reenvían deliberadamente el mismo request o evento varias veces (webhooks duplicados, mensajes reprocesados de una cola) y verifican que el resultado final sea idéntico a una sola ejecución.

**Para qué sirve:** confirmar que operaciones críticas (cobros, creación de recursos, registro de eventos) no se dupliquen cuando algo se reintenta o reenvía.

**Por qué importa:** sin esto, un reintento de red o un webhook duplicado puede generar un cobro doble, un registro duplicado o un archivo duplicado — errores costosos y difíciles de revertir.

**Herramientas:** no es una herramienta específica, es una categoría de test que se escribe explícitamente contra cada operación marcada como idempotente.

**Etapa de implementación:** **Antes de manejar dinero o eventos externos en producción** — imprescindible antes del primer cobro real o del primer webhook de un proveedor de pagos.

---

## 1.5 Snapshot testing

**Qué es:** congela la salida de una función o respuesta y detecta cualquier cambio no intencional frente a esa referencia guardada.

**Para qué sirve:** proteger formatos de API, facturas, o payloads de eventos de cambios accidentales al refactorizar código.

**Por qué importa:** cambios sutiles de formato pueden romper integraciones de clientes o generar inconsistencias en documentos legales/contables sin que se note en el código.

**Herramientas:** `insta`.

**Etapa de implementación:** **Desarrollo temprano**, apenas existan formatos de salida estables que deban mantenerse consistentes (facturas, eventos, respuestas de API pública).

---

## 1.6 Concurrency testing con Loom

**Qué es:** simula exhaustivamente todos los entrelazados posibles de hilos/tareas concurrentes/async para encontrar condiciones de carrera.

**Para qué sirve:** verificar garantías de concurrencia (ej. "una cuota nunca puede excederse por una carrera entre dos subidas simultáneas") de forma exhaustiva, no solo lanzando N requests en paralelo y esperando que salga bien.

**Por qué importa:** las condiciones de carrera reales a veces solo aparecen 1 de cada 10,000 ejecuciones — son casi imposibles de atrapar con pruebas convencionales, y suelen causar los bugs más difíciles de reproducir en producción.

**Herramientas:** `loom`.

**Etapa de implementación:** **Antes de producción**, específicamente en cualquier sección del código que reserve/modifique un recurso compartido bajo concurrencia (cuotas, contadores, balances, inventario).

---

## 1.7 Mutation testing (pruebas de mutación)

**Qué es:** modifica automáticamente el código de producción (invierte condiciones, cambia operadores) y verifica si los tests existentes detectan el cambio.

**Para qué sirve:** confirmar que la cobertura de tests es real y no solo "tests que pasan sin probar nada significativo".

**Por qué importa:** un alto porcentaje de cobertura de código puede ser una ilusión si los tests no tienen asserts que realmente detecten errores.

**Herramientas:** `cargo-mutants`.

**Etapa de implementación:** **Fase de madurez**, cuando ya existe una suite de tests considerable y se quiere validar su calidad real — no tiene sentido aplicarlo antes de tener tests que mutar.

---

## 1.8 Fuzzing

**Qué es:** bombardea funciones con entradas aleatorias o malformadas buscando panics, crashes o comportamiento indefinido.

**Para qué sirve:** encontrar vulnerabilidades en cualquier código que procese input externo no confiable (parsers de archivos, deserialización de webhooks, validación de tokens).

**Por qué importa:** es la superficie de ataque más común explotada en sistemas reales — parsers mal preparados para input malicioso o corrupto.

**Herramientas:** `cargo-fuzz` (basado en libFuzzer).

**Etapa de implementación:** **Antes de producción**, obligatorio en cualquier parser que reciba archivos o datos de usuarios/terceros no confiables.

---

## 1.9 Miri (detección de undefined behavior)

**Qué es:** intérprete que ejecuta tests detectando comportamiento indefinido de memoria (accesos inválidos, data races en código `unsafe`).

**Para qué sirve:** detectar errores de memoria que ni el compilador ni los tests normales pueden ver.

**Por qué importa:** solo relevante si el proyecto usa bloques `unsafe`; si no se usa `unsafe` en absoluto, esta prueba es de baja prioridad.

**Herramientas:** `cargo +nightly miri test`.

**Etapa de implementación:** **Condicional** — tan pronto se introduzca el primer bloque `unsafe` en el código, no antes.

---

## 1.10 Contract testing (pruebas de contrato)

**Qué es:** verifica que el contrato de comunicación entre servicios (gRPC/Protobuf, REST/OpenAPI) no se rompa cuando un servicio cambia, sin necesitar levantar todos los servicios juntos.

**Para qué sirve:** detectar incompatibilidades entre microservicios antes del despliegue.

**Por qué importa:** en arquitecturas de microservicios, un cambio "inofensivo" en un servicio puede romper silenciosamente a otro que depende de su contrato exacto.

**Herramientas:** `pact` (bindings Rust), validación de compatibilidad de esquema con `buf breaking` (Protobuf).

**Etapa de implementación:** **Antes de producción**, apenas exista más de un servicio comunicándose por un contrato versionado.

---

## 1.11 Load / Stress / Soak testing

**Qué es:** tres variantes de pruebas de rendimiento bajo carga:
- **Load testing:** cuánta carga sostiene el sistema en tráfico esperado.
- **Stress testing:** en qué punto exacto colapsa (para conocer el techo real).
- **Soak testing:** correr carga moderada por horas/días buscando fugas de memoria o degradación gradual.

**Para qué sirve:** conocer la capacidad real del sistema y detectar problemas que solo aparecen con el tiempo (leaks, agotamiento de pools de conexión).

**Por qué importa:** sin esto, la capacidad real del sistema es una suposición, no un dato medido — el primer stress test real termina siendo un incidente en producción.

**Herramientas:** `goose` (nativo Rust), `k6`, `drill`.

**Etapa de implementación:** **Antes de producción**, con foco inicial en load testing; soak testing se puede ejecutar en paralelo al crecimiento del tráfico real.

---

## 1.12 Pruebas de caos (Chaos Engineering)

**Qué es:** inyecta fallos deliberados en un sistema en ejecución (matar procesos, cortar red, saturar recursos, introducir latencia) para observar si el sistema se recupera solo o colapsa.

**Para qué sirve:** validar que los mecanismos de resiliencia (reintentos, timeouts, colas con DLQ, réplicas) funcionan de verdad bajo fallo real, no solo en teoría.

**Por qué importa:** es la diferencia entre "documenté que el sistema reintenta" y "confirmé que reintenta cuando algo se rompe a mitad de un proceso crítico".

**Herramientas:** `toxiproxy` (latencia/cortes de red), Chaos Mesh o Litmus (Kubernetes), scripts de `kill -9` controlados en staging.

**Etapa de implementación:** **Fase de madurez, en staging primero.** Requiere un sistema ya funcional con mecanismos de resiliencia implementados — no tiene sentido aplicarlo antes de tener algo que resista al caos.

---

## 1.13 Pruebas de integridad de datos

**Qué es:** verifican que los datos permanecen correctos y consistentes a través del tiempo y de operaciones concurrentes — que "los números cuadran", no solo que "el código no truena".

**Para qué sirve:**
- **Integridad contable:** que cargos, reintentos y estados de facturación siempre reconstruyan una historia coherente.
- **Integridad de cuota:** que el uso registrado nunca exceda el límite real, detectando desincronización ("drift") entre el contador y la realidad.
- **Integridad referencial entre sistemas:** que un dato en un sistema (ej. base vectorial) siempre tenga su contraparte válida en otro (ej. base relacional), sin huérfanos en ninguna dirección.

**Por qué importa:** un sistema puede funcionar sin errores visibles y aun así tener datos silenciosamente incorrectos — el tipo de bug que solo se descubre en una auditoría o una queja de cliente meses después.

**Herramientas:** jobs de reconciliación periódicos (recalculan el estado real y lo comparan contra el cacheado/registrado) + `proptest` aplicado a invariantes de negocio tras secuencias aleatorias de operaciones.

**Etapa de implementación:** **Antes de producción con datos reales de clientes**, especialmente si hay facturación, cuotas o dos almacenes de datos que deben mantenerse sincronizados.

---

## 1.14 Security testing (SAST / dependencias)

**Qué es:** análisis estático de código y auditoría de dependencias en busca de vulnerabilidades conocidas, licencias prohibidas o uso riesgoso de memoria.

**Para qué sirve:** prevenir vulnerabilidades introducidas por terceros (dependencias) o por patrones de código riesgosos.

**Por qué importa:** con datos de usuario o pagos en juego, una dependencia vulnerable sin detectar es una puerta de entrada directa a un incidente de seguridad.

**Herramientas:** `clippy` (incluyendo `clippy::pedantic`), `cargo-audit`, `cargo-deny`, `cargo-geiger`.

**Etapa de implementación:** **Día 0.** Barato de configurar, alto valor — debe correr en cada Pull Request desde el primer commit.

---

## 1.15 Disaster recovery / restore drills

**Qué es:** ejercicio operativo real (no automatizado en código) de borrar deliberadamente una réplica de datos en staging y cronometrar cuánto tarda la restauración siguiendo el runbook documentado.

**Para qué sirve:** confirmar que el proceso de recuperación ante desastres funciona de verdad, no solo que existe como documento.

**Por qué importa:** un runbook nunca probado es, en la práctica, un runbook que no funciona — se descubre en el peor momento posible si no se ensaya antes.

**Herramientas:** ninguna específica; es un ejercicio operativo con checklist y cronómetro.

**Etapa de implementación:** **Obligatorio antes de producción con clientes de pago**, y repetido periódicamente (ej. cada trimestre) después.

---

## 1.16 Smoke tests post-deploy

**Qué es:** set pequeño y rápido de verificaciones que corren automáticamente después de cada despliegue a producción, confirmando que lo esencial sigue vivo.

**Para qué sirve:** detectar de inmediato si un despliegue rompió algo crítico (login, flujo de pago, una consulta clave), sin esperar a que un usuario lo reporte.

**Por qué importa:** reduce drásticamente el tiempo entre "algo se rompió" y "nos dimos cuenta".

**Herramientas:** scripts propios de smoke test integrados al pipeline de CI/CD.

**Etapa de implementación:** **Antes del primer despliegue a producción real.**

---

## 1.17 Pruebas end-to-end (E2E)

**Qué es:** pruebas que ejecutan un flujo completo desde el punto de entrada
real del sistema hasta su resultado observable, atravesando frontend o cliente
HTTP, BFF, autenticación, microservicios, PostgreSQL, Redis, NATS JetStream,
R2, embeddings y Qdrant. No validan únicamente una función ni un contrato
aislado: validan que el sistema completo cumple el caso de uso.

**Para qué sirve:** confirmar que los componentes desplegados, sus
configuraciones, credenciales, contratos y eventos funcionan juntos en el
mismo recorrido que utilizará un cliente. También detecta errores que no
aparecen en pruebas unitarias, de integración o de contrato por separado.

**Flujos mínimos obligatorios para este sistema:**

- Registro, verificación de correo, inicio de sesión, renovación de sesión y
  cierre de sesión.
- Creación de una suscripción en PayPal Sandbox, confirmación del webhook,
  activación del plan, cancelación y bloqueo posterior cuando corresponda.
- Creación, rotación y revocación de una API key comprobando scopes separados
  para modelos, archivos y almacenamiento vectorial.
- Subida de un PDF, DOCX, XLSX y TXT; generación de URL firmada; confirmación
  del archivo; publicación Outbox → NATS JetStream; ingestión; generación del
  embedding Qwen3; escritura canónica en Qdrant y actualización del estado en
  PostgreSQL.
- Consulta RAG autenticada y verificación de que solo devuelve documentos del
  usuario propietario, con trazabilidad completa mediante `request_id` y
  `trace_id`.
- Renombrado, movimiento, descarga y borrado de archivos; confirmación del
  borrado coordinado de referencias y vectores canónicos.
- Acceso S3 compatible del Pro Plus mediante SigV4, incluyendo subida,
  descarga, rangos y multipart upload.
- Flujo administrativo de consulta, bloqueo, activación y auditoría de un
  usuario sin exponer secretos ni datos de otro usuario.
- Reintento/redelivery de eventos, expiración de lease, error del proveedor,
  DLQ y recuperación sin duplicar cargos, archivos, embeddings ni vectores.

**Criterios de seguridad E2E:**

- Ejecutar al menos con dos usuarios aislados y comprobar que A no puede leer,
  buscar, descargar, reindexar ni borrar datos de B.
- Ignorar o rechazar `user_id`, colección, filtros Qdrant y point IDs enviados
  por el cliente; la identidad debe provenir exclusivamente de la sesión o
  API key autenticada.
- No incluir tokens, contraseñas, contenido de documentos ni PII innecesaria
  en logs, trazas, reportes o capturas de la prueba.
- Verificar que los endpoints internos usan mTLS/JWT de servicio y que un
  servicio no autorizado recibe un rechazo auditable.

**Ejecución y herramientas:** usar el entorno de staging con dependencias
reales y datos efímeros identificables por `run_id`; Playwright para recorridos
del navegador cuando exista UI y `reqwest`/clientes gRPC para recorridos de
API. PayPal debe ejecutarse en Sandbox y nunca contra cobros reales. Los
eventos se esperan mediante consultas de estado, consumer ACK o condiciones
observables; no usar `sleep()` fijo para esperar propagación. Cada ejecución
debe conservar un reporte con pasos, duración, estado, `run_id`,
`request_id`/`trace_id`, versión desplegada y evidencia de limpieza.

**Etapa de implementación:** **Obligatoria antes del primer despliegue con
clientes de pago y después de cada cambio que atraviese más de un servicio.**
El conjunto crítico debe ejecutarse también como smoke E2E posterior a cada
despliegue.

---

# PARTE 2 — OBSERVABILIDAD

## 2.1 Métricas, trazas OTEL, dashboards y alertas base

**Qué es:** instrumentación estándar con OpenTelemetry (métricas y trazas), almacenadas y visualizadas típicamente con Prometheus/Grafana/Loki/Tempo, con alertas configuradas sobre condiciones anómalas.

**Para qué sirve:** dar visibilidad básica de salud del sistema — qué tan rápido responde, cuántos errores ocurren, dónde se pierde tiempo en una petición distribuida entre servicios.

**Por qué importa:** sin esto, cualquier incidente se investiga a ciegas, adivinando en vez de midiendo.

**Herramientas:** OpenTelemetry SDK, Prometheus, Grafana, Loki (logs), Tempo (trazas).

**Etapa de implementación:** **Día 0.** La instrumentación (métricas + trazas + logs estructurados) debe integrarse en el esqueleto/plantilla base de cada servicio desde su creación — **verificar explícitamente que todo servicio nuevo quede incluido en la instrumentación común**, no agregarla después como parche. Retrofitear observabilidad en un sistema ya grande es mucho más costoso que incluirla desde el primer servicio.

---

## 2.2 SLOs y error budgets

**Qué es:** un Service Level Objective formal (ej. "99.5% de requests deben responder en menos de 500ms durante 30 días"), a partir del cual se calcula automáticamente cuánto margen de fallo ("error budget") queda disponible.

**Para qué sirve:** darle un propósito accionable a las métricas de latencia/error (p50/p95/p99) en vez de que sean solo números que nadie revisa hasta que ya es tarde.

**Por qué importa:** permite decidir objetivamente cuándo frenar el ritmo de despliegues porque se está agotando el margen de fallo permitido, en vez de reaccionar solo cuando ya hay una crisis.

**Herramientas:** `Sloth`, `Pyrra` (generan reglas de alerta de Prometheus a partir de una definición de SLO).

**Etapa de implementación:** **Antes de producción**, en cuanto existan endpoints críticos cuya latencia/disponibilidad importe contractualmente o para la experiencia de usuario.

---

## 2.3 Continuous profiling (perfilado continuo)

**Qué es:** captura constante en producción de qué función/línea de código consume CPU o memoria, más allá de lo que muestra una traza.

**Para qué sirve:** llegar directo a la causa exacta de un cuello de botella (ej. un hot loop ineficiente), no solo saber qué servicio tardó más.

**Por qué importa:** una traza dice "qué" tardó; el profiling dice "por qué" — sin esto, diagnosticar problemas de rendimiento sutiles depende de reproducir el problema manualmente.

**Herramientas:** `Grafana Pyroscope` (se integra con Tempo vía exemplars).

**Etapa de implementación:** **Fase de madurez**, cuando el sistema ya tiene tráfico real suficiente para que el profiling continuo aporte señal útil.

---

## 2.4 Exemplars (puente entre métricas y trazas)

**Qué es:** mecanismo que adjunta a un punto de una métrica (ej. un pico de latencia) el `trace_id` exacto de la traza que lo generó.

**Para qué sirve:** saltar directo desde un gráfico de métricas a la traza específica responsable, sin búsqueda manual.

**Por qué importa:** ahorra tiempo crítico de diagnóstico durante un incidente activo.

**Herramientas:** soporte nativo de exemplars en Prometheus + Grafana + Tempo.

**Etapa de implementación:** **Junto con la instrumentación base (2.1)** si el stack lo soporta desde el inicio; si no, se agrega en fase de madurez sin mayor fricción.

---

## 2.5 Observabilidad específica de colas de mensajes

**Qué es:** métricas dedicadas de consumer lag (mensajes pendientes de procesar), profundidad de dead-letter queue (DLQ), y tasa de reintentos por stream/consumidor.

**Para qué sirve:** detectar fallos sistemáticos en el procesamiento asíncrono (ej. un tipo de archivo que siempre falla al procesarse) antes de que un cliente lo reporte.

**Por qué importa:** una DLQ creciendo silenciosamente es una señal temprana de un problema que, sin monitoreo específico, pasa desapercibido hasta acumularse en algo grave.

**Herramientas:** exporter nativo de la cola de mensajes usada (ej. NATS JetStream, Kafka, RabbitMQ) hacia Prometheus + alertas de umbral en profundidad de DLQ y lag de consumidor.

**Etapa de implementación:** **Antes de producción**, tan pronto exista un pipeline asíncrono con colas y reintentos.

---

## 2.6 Observabilidad de base de datos

**Qué es:** métricas de queries lentas, uso del pool de conexiones, locks y tiempos de espera en transacciones.

**Para qué sirve:** ver en tiempo real si el pool de conexiones se agota o si hay locks de larga duración compitiendo por las mismas filas/recursos.

**Por qué importa:** especialmente crítico en sistemas con transacciones atómicas para cuotas/balances — sin esto, un cuello de botella de base de datos se percibe como "lentitud general" sin poder localizar la causa.

**Herramientas:** `pg_stat_statements` (PostgreSQL) + `postgres_exporter` para Prometheus (equivalentes existen para otros motores de base de datos).

**Etapa de implementación:** **Antes de producción**, en cuanto la base de datos maneje operaciones concurrentes críticas (cuotas, pagos, inventario).

---

## 2.7 Alertas de cardinalidad / control de costo del propio stack de observabilidad

**Qué es:** monitoreo de que las propias métricas/logs no exploten en cardinalidad (ej. usar un identificador de usuario como label de una métrica, generando miles de series únicas).

**Para qué sirve:** evitar que el sistema de monitoreo se vuelva lento o caro sin que nadie note la causa.

**Por qué importa:** es un error común y silencioso — el stack de observabilidad, pensado para ayudar, termina siendo un problema de costo/rendimiento en sí mismo.

**Herramientas:** alertas propias sobre el número de series activas en Prometheus/Mimir.

**Etapa de implementación:** **Fase de madurez**, cuando el volumen de métricas/usuarios empieza a crecer — revisar cardinalidad desde el diseño inicial de cada métrica ayuda a no necesitar esto con urgencia después.

---

## 2.8 Synthetic monitoring / uptime checks

**Qué es:** un proceso externo que simula a un usuario real periódicamente (login, navegación, incluso un pago de prueba en sandbox), sin depender de tráfico real para generar señal.

**Para qué sirve:** detectar caídas o fallos incluso cuando no hay usuarios activos generando métricas en ese momento (ej. de madrugada).

**Por qué importa:** las métricas internas solo dicen algo si hay tráfico real produciéndolas; sin monitoreo sintético, un fallo con cero tráfico pasa desapercibido hasta que el primer usuario del día lo sufre.

**Herramientas:** `Grafana Synthetic Monitoring`, `Blackbox Exporter`.

**Etapa de implementación:** **Antes de producción**, cubriendo como mínimo el flujo crítico de negocio (login, checkout/pago, la acción principal del producto).

---

## 2.9 Dashboards de negocio

**Qué es:** paneles con métricas de negocio (no técnicas) en la misma herramienta de observabilidad — ingresos recurrentes, conversión de prueba a pago, cancelaciones, distribución de usuarios por plan.

**Para qué sirve:** dar visibilidad en tiempo real de la salud del negocio, no solo de la infraestructura.

**Por qué importa:** sin esto, las decisiones de negocio dependen de reportes manuales desactualizados en vez de datos en vivo.

**Herramientas:** el mismo Grafana ya usado para métricas técnicas, alimentado con datos del sistema de suscripciones/facturación.

**Etapa de implementación:** **Apenas exista el primer flujo de pago/suscripción en producción.**

---

## 2.10 Dashboard / alertas de costo (FinOps)

**Qué es:** métricas específicas de gasto real desglosado por componente (proveedores externos de cómputo/API, almacenamiento, etc.), por plan y por usuario — no solo el total de la factura a fin de mes.

**Para qué sirve:** detectar en tiempo real si un usuario o componente específico está generando un costo anómalo, antes de que se acumule en una pérdida mensual significativa.

**Por qué importa:** especialmente crítico en modelos de negocio con "uso justo" o límites blandos, donde un solo caso de abuso o mal uso puede erosionar el margen sin que nadie lo note hasta la factura del proveedor.

**Herramientas:** dashboards propios en Grafana alimentados por métricas de consumo real de cada proveedor externo, correlacionadas con el `user_id`/plan.

**Etapa de implementación:** **Apenas el sistema dependa de proveedores externos cobrados por uso** (APIs de terceros, cómputo, almacenamiento).

---

## 2.11 Integración con gestión de incidentes (on-call)

**Qué es:** conexión de las alertas del sistema con una herramienta de guardia/escalamiento que define quién recibe cada alerta y cómo escala si no hay respuesta.

**Para qué sirve:** asegurar que una alerta crítica tenga un responsable claro y un tiempo de reacción definido.

**Por qué importa:** una alerta que nadie ve a tiempo es equivalente a no tener alerta.

**Herramientas:** `PagerDuty`, `Opsgenie`, `Grafana OnCall`.

**Etapa de implementación:** **En cuanto el sistema tenga usuarios reales dependiendo de su disponibilidad**, y exista más de una persona en el equipo (para que el escalamiento tenga sentido).

---

## 2.12 Runbooks vinculados a cada alerta

**Qué es:** práctica (no herramienta) de vincular cada alerta a un documento corto con los primeros pasos a seguir cuando dispara.

**Para qué sirve:** que cualquier persona del equipo pueda reaccionar consistentemente ante un incidente, sin depender de que alguien específico recuerde el procedimiento.

**Por qué importa:** sin runbooks, cada incidente se investiga desde cero, alargando el tiempo de resolución y dependiendo de memoria individual.

**Herramientas:** documentación versionada, enlazada directamente desde la configuración de cada alerta.

**Etapa de implementación:** **Junto con cada alerta que se cree** — la práctica correcta es no crear una alerta sin su runbook asociado, desde el principio.

---

# PARTE 3 — ROADMAP DE PRIORIZACIÓN POR ETAPA

Esta tabla combina testing y observabilidad en una sola línea de tiempo, pensada para no tener que rediseñar nada más adelante.

## Etapa 0 — Día 0 / Fundación del proyecto

*Barato de establecer ahora, muy costoso de retrofitear después.*

| Punto | Sección |
|---|---|
| Pruebas unitarias | 1.1 |
| `cargo-audit` + `cargo-deny` + `clippy` en CI | 1.14 |
| Instrumentación OTEL base (métricas + trazas + logs) en el esqueleto de cada servicio | 2.1 |
| Exemplars (si el stack los soporta desde el inicio) | 2.4 |

## Etapa 1 — Desarrollo temprano (primeras features funcionando)

| Punto | Sección |
|---|---|
| Pruebas de integración | 1.2 |
| Property-based testing | 1.3 |
| Snapshot testing | 1.5 |

## Etapa 2 — Antes de producción / antes del primer cliente de pago

*El sistema ya maneja dinero, datos reales o concurrencia crítica.*

| Punto | Sección |
|---|---|
| Idempotency / replay testing | 1.4 |
| Loom (concurrencia) | 1.6 |
| Fuzzing en parsers de input externo | 1.8 |
| Contract testing | 1.10 |
| Load / stress testing | 1.11 |
| Pruebas de integridad de datos | 1.13 |
| Pruebas end-to-end (E2E) | 1.17 |
| Disaster recovery / restore drill (al menos una vez) | 1.15 |
| Smoke tests post-deploy | 1.16 |
| SLOs y error budgets | 2.2 |
| Observabilidad de colas de mensajes | 2.5 |
| Observabilidad de base de datos | 2.6 |
| Synthetic monitoring | 2.8 |
| Dashboards de negocio | 2.9 |
| Dashboard de costo / FinOps | 2.10 |
| Runbooks por cada alerta creada | 2.12 |

## Etapa 3 — Producción temprana con usuarios reales

| Punto | Sección |
|---|---|
| Soak testing sostenido | 1.11 |
| Integración on-call | 2.11 |

## Etapa 4 — Fase de madurez / escala

*El sistema ya tiene tráfico y datos suficientes para que estas prácticas aporten señal real.*

| Punto | Sección |
|---|---|
| Mutation testing | 1.7 |
| Miri (solo si hay `unsafe`) | 1.9 |
| Chaos Engineering (empezando en staging) | 1.12 |
| Restore drills recurrentes (ej. trimestral) | 1.15 |
| Continuous profiling | 2.3 |
| Alertas de cardinalidad | 2.7 |

---

## Principio general para cualquier proyecto futuro

1. **Lo barato de establecer temprano (Etapa 0) nunca se pospone** — instrumentación base y seguridad de dependencias cuestan casi nada al inicio y muchísimo si se agregan sobre un sistema ya grande.
2. **Lo que protege dinero y datos reales (Etapa 2) es la barrera antes de aceptar el primer cliente de pago**, no un "nice to have" posterior.
3. **Lo que requiere tráfico o escala real (Etapa 3-4) no se implementa antes de tenerlos** — hacerlo antes es esfuerzo sin señal útil que retrasa lo que sí importa en etapas tempranas.
4. **Ninguna categoría reemplaza a las demás.** Testing prueba que el código es correcto; observabilidad prueba que el sistema en producción se comporta como se espera. Un sistema robusto necesita ambas, en el orden correcto.
