# Monitoreo de NATS JetStream

## Endpoints

- Cliente NATS: `nats://100.121.34.76:4223`
- Health HTTP: `http://100.121.34.76:8223/healthz`
- Varz: `http://100.121.34.76:8223/varz`
- JetStream info: `http://100.121.34.76:8223/jsz?config=1&accounts=1`

## Seguridad

- La autenticación por `username/password` es obligatoria.
- TLS se activa con `NATS_TLS_ENABLED=false`.
- La configuración viva se genera desde `./.env` y `script/start-nats.sh`.

## Alta disponibilidad

- `NATS_CLUSTER_ENABLED=false`: nodo standalone normal.
- `NATS_CLUSTER_ENABLED=true`: este nodo entra al mesh de cluster usando `routes`.
- Para HA fuerte de JetStream con persistencia, usa `3` nodos.
- Con `2` nodos puedes tener conexiones activas en paralelo, pero no tienes el mismo nivel de quorum durable que con 3.
- Para distribuir entre VPS, corre el mismo `docker-compose.nats.yml` en cada VPS con un `.env` distinto.

## Checks rapidos

```bash
curl -s http://127.0.0.1:8223/healthz
curl -s http://127.0.0.1:8223/varz | jq
curl -s 'http://127.0.0.1:8223/jsz?config=1&accounts=1' | jq
nats --server nats://nats_admin:tu_password@127.0.0.1:4223 account info
```

## Volumen persistente

Los datos de JetStream quedan en:

```text
./volumes/<node_id>/data
```

## Variables críticas de capacidad

- `NATS_JETSTREAM_MAX_FILE_STORE`: techo global del almacenamiento persistente.
- `NATS_JETSTREAM_MAX_MEMORY_STORE`: techo global del almacenamiento en memoria.
- `NATS_MAX_CONNECTIONS`: número máximo de clientes simultáneos.
- `NATS_MAX_SUBSCRIPTIONS`: fanout máximo por conexión.
- `NATS_CONTAINER_CPUS`: CPU reservada para cada nodo.
- `NATS_CONTAINER_MEMORY`: memoria máxima del contenedor.
- `NATS_CONTAINER_PIDS_LIMIT`: máximo de procesos/threads del contenedor.

## Variables críticas de retención

La retención por tiempo y bytes de cada stream la definen las aplicaciones:

- `automatizacion/.env`
  - `NATS_TASK_STREAM_MAX_AGE`
  - `NATS_TASK_STREAM_MAX_BYTES`
  - `NATS_RESULT_RETENTION`
  - `NATS_RESULT_STREAM_MAX_BYTES`
- `microservicios/email-service-managed/config/.env`
  - `EMAIL_JETSTREAM_MAX_AGE`
  - `EMAIL_JETSTREAM_MAX_BYTES`

## Notas operativas

- Este NATS compartido está pensado para la plataforma completa.
- No debe ser levantado desde un microservicio.
- Cambios de seguridad/capacidad se hacen en:
  - `./.env`
  - `./script/start-nats.sh`
- Ejemplo cluster multi-VPS:
  - `cp .env.cluster.example .env`
  - ajusta `NATS_NODE_ID`, `NATS_CLUSTER_ADVERTISE_HOST` y `NATS_CLUSTER_ROUTES`
  - levanta un nodo por VPS con `docker compose --env-file .env -f docker-compose.nats.yml up -d`
- Archivos listos por nodo:
  - `node1.env`
  - `node2.env`
  - `node3.env`
- Comandos:
```bash
docker compose --env-file node1.env -f docker-compose.nats.yml up -d
docker compose --env-file node2.env -f docker-compose.nats.yml up -d
docker compose --env-file node3.env -f docker-compose.nats.yml up -d
```
- Guia operativa completa:
  - `doc/CLUSTER_OPERACION.md`
