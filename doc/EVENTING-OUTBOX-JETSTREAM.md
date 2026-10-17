# Outbox → NATS JetStream

> Reconciliado con `crates/events` y los consumidores actuales el 2026-07-25.

## Alcance y garantías

NATS JetStream es el único backbone durable. No se usa Kafka.

La entrega es **at-least-once**:

1. El cambio de negocio y su `EventEnvelope` se insertan en la misma transacción PostgreSQL cuando el servicio tiene una operación SQL asociada.
2. El relay reclama filas mediante `FOR UPDATE SKIP LOCKED`.
3. Publica a JetStream con `Nats-Msg-Id = outbox.id`.
4. Sólo marca la fila como `published` después de recibir el PubAck de JetStream.
5. Si el PubAck se pierde, la fila se reintenta. La ventana de deduplicación de JetStream evita publicaciones duplicadas durante 24 horas.
6. Los consumidores confirman con double ACK. La lógica de dominio consumidora debe seguir siendo idempotente porque un fallo entre el commit local y el ACK puede causar una nueva entrega.

Los eventos de telemetría de `api-gateway` y el estado Redis de `model-router` no tienen una transacción PostgreSQL de negocio que compartir. En esos dos casos, la inserción en Outbox es la operación durable primaria inmediatamente posterior al cambio externo.

## Streams y subjects

| Stream | Subjects | Retención |
|---|---|---|
| `EVENTS` | Los 14 subjects canónicos de `crates/events` | 30 días, 1 millón de mensajes o 1 GiB |
| `EVENTS_DLQ` | `dlq.>` | 90 días, 100 mil mensajes o 512 MiB |

Subjects canónicos:

```text
usage.recorded
payment.succeeded
payment.failed
account.marked_unhealthy
account.recovered
wallet.debited
wallet.low_balance
billing.period_closed
subscription.changed
api_key.revoked
api_key.authorization_changed
request.failed
file.ingest.requested
file.vectors.delete.requested
```

No se deben agregar prefijos como `events.`. Los subjects DLQ tienen la forma:

```text
dlq.<consumer>.<subject-original>
```

## Productores

| Servicio | Tabla | Integración transaccional |
|---|---|---|
| api-gateway | `gateway.event_outbox` | Inserción durable de telemetría |
| model-router | `router.event_outbox` | Inserción posterior al cambio en Redis |
| credential-manager | `credentials.event_outbox` | Misma transacción que `update_health` |
| key-services | `keys.event_outbox` | Misma transacción que la revocación |
| payment-services | `payments.event_outbox` | Misma transacción que el cambio de estado |
| subscription-service | `subscription.event_outbox` | Misma transacción que create/update |
| billing-service | `billing.event_outbox` | Misma transacción que el débito; cierre de período como comando durable |
| key-services | `event_outbox` | Misma transacción que revocación o cambio de autorización |
| file-service | `event_outbox` | Misma transacción que confirmación, reindexado o borrado lógico |

Cada relay se inicia como una tarea Tokio del proceso productor. Las filas usan estos estados:

```text
pending -> processing -> published
                    \-> pending  (reintento con backoff)
                    \-> failed   (OUTBOX_MAX_ATTEMPTS agotado)
```

Una fila `processing` cuyo lease expiró puede ser reclamada por otra instancia. Las filas terminales se pueden reencolar de forma explícita:

```sql
UPDATE event_outbox
SET status = 'pending',
    attempts = 0,
    available_at = now(),
    locked_at = NULL,
    last_error = NULL
WHERE id = '<event-id>' AND status = 'failed';
```

## Consumidores

| Consumidor durable | Subject |
|---|---|
| `usage-service` | `usage.recorded` |
| `billing-usage-consumer` | `usage.recorded` |
| `billing-payment-consumer` | `payment.succeeded` |
| `invoice-service` | `billing.period_closed` |
| `api-gateway-key-revocations` | `api_key.revoked` |
| `api-gateway-key-authorization` | `api_key.authorization_changed` |
| `file-ingestion-worker` | `file.ingest.requested` |
| `file-vector-delete-worker` | `file.vectors.delete.requested` |

Todos son pull consumers con `AckPolicy::Explicit`, `filter_subject`, `ack_wait=30s` y backoff. El proceso de negocio se intenta como máximo cinco veces; el límite de mensajes pendientes y el tamaño de lote de los workers de archivos se configuran mediante `FILE_SERVICE_INGESTION_MAX_ACK_PENDING`, `FILE_SERVICE_INGESTION_BATCH_SIZE`, `FILE_SERVICE_VECTOR_DELETE_MAX_ACK_PENDING` y `FILE_SERVICE_VECTOR_DELETE_BATCH_SIZE`.

- Éxito o duplicado idempotente: double ACK.
- Error transitorio antes del quinto intento: NAK.
- Quinto error: publicar el `DeadLetter`, esperar su PubAck y terminar el mensaje original.
- Sobre inválido: DLQ inmediata; no se reintenta la lógica de negocio.
- Si falla la publicación a DLQ, JetStream reserva cinco entregas adicionales exclusivamente para reintentar la escritura en DLQ.

El arranque usa create/update para corregir consumidores durables que existieran con una configuración anterior sin filtro.

## Formato DLQ

```json
{
  "id": "uuid-del-evento-o-dlq",
  "consumer": "usage-service",
  "original_subject": "usage.recorded",
  "delivery_attempts": 5,
  "error": "causa final",
  "payload": {},
  "failed_at": "2026-07-23T00:00:00Z"
}
```

Antes de reinyectar un mensaje, se debe corregir la causa, verificar que el consumidor sea idempotente y publicar únicamente `payload` en `original_subject`. No se debe borrar el mensaje DLQ hasta confirmar el procesamiento correcto.

## Configuración

```env
OUTBOX_POLL_INTERVAL_MS=500
OUTBOX_BATCH_SIZE=100
OUTBOX_MAX_ATTEMPTS=10
OUTBOX_LEASE_SECONDS=60
```

Las migraciones nuevas deben aplicarse antes de arrancar los productores. Para desarrollo:

```bash
./run-all.sh
```

O manualmente:

```bash
DATABASE_URL="$SERVICE_DATABASE_URL" \
  sqlx migrate run --no-dotenv --source apps/<servicio>/migrations
```

Cada bounded context usa su propia base de datos y su tabla
`public._sqlx_migrations`. Esto evita colisiones entre versiones de migración
aunque las bases compartan el mismo contenedor PostgreSQL.

## Operación

Consultas mínimas:

```sql
SELECT status, count(*)
FROM event_outbox
GROUP BY status;

SELECT id, event_type, attempts, last_error, created_at
FROM event_outbox
WHERE status = 'failed'
ORDER BY created_at;
```

Métricas:

- `outbox_enqueued_total`
- `outbox_published_total`
- `outbox_failed_total`
- `outbox_backlog`
- `events_dlq_total`

Los endpoints `/metrics` de billing, usage e invoice exponen `events_dlq_total` para los consumidores implementados.
