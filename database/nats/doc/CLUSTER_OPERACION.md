# Operacion del Cluster NATS JetStream

## Arquitectura recomendada

- `1` nodo: desarrollo o laboratorio.
- `3` nodos: produccion con alta disponibilidad real.
- `2` nodos: no recomendado para quorum fuerte de JetStream.

## Archivos listos

- [node1.env](/media/servidor-nitro/ARCHIVOS-KEVINS/archivos-kevins/desarrollo/n8n-automatizacion/stack/database/nats/node1.env)
- [node2.env](/media/servidor-nitro/ARCHIVOS-KEVINS/archivos-kevins/desarrollo/n8n-automatizacion/stack/database/nats/node2.env)
- [node3.env](/media/servidor-nitro/ARCHIVOS-KEVINS/archivos-kevins/desarrollo/n8n-automatizacion/stack/database/nats/node3.env)

## Preparacion por VPS

1. Copia la carpeta `stack/database/nats`.
2. Genera o distribuye los certificados en `ssl/`.
3. En cada VPS usa su archivo `nodeX.env`.
4. Ajusta `NATS_CLUSTER_ADVERTISE_HOST` y `NATS_CLUSTER_ROUTES` a las IPs reales.

## Arranque

En cada VPS:

```bash
cd stack/database/nats
docker compose --env-file node1.env -f docker-compose.nats.yml up -d
```

Cambia `node1.env` por `node2.env` o `node3.env` segun el VPS.

## Verificacion local

```bash
curl -s http://127.0.0.1:8223/healthz
curl -s http://127.0.0.1:8223/varz | jq
curl -s 'http://127.0.0.1:8223/jsz?config=1&accounts=1' | jq
```

## Verificacion de cluster

Desde cualquier nodo:

```bash
nats --server nats://nats_admin:TU_PASSWORD@127.0.0.1:4223 server report jetstream
nats --server nats://nats_admin:TU_PASSWORD@127.0.0.1:4223 server check
```

## Streams durables en HA

Para que la persistencia tambien sea HA y no solo el transporte:

- En `automatizacion/.env` usa `NATS_STREAM_REPLICAS=3`
- En `microservicios/email-service-managed/config/.env` usa `EMAIL_JETSTREAM_REPLICAS=3`

Si el cluster corre con un solo nodo, deja esas variables en `1`.

## Failover esperado

- Si cae un nodo y quedan `2/3`, el cluster sigue operativo.
- Si cae el nodo que atendia una conexion cliente, los clientes deben reconectar a otro nodo del mesh.
- Si tus aplicaciones apuntan a un solo `NATS_URL`, conviene poner delante un LB TCP o una lista de endpoints en el futuro.

## Cambio de standalone a cluster

1. Levanta los 3 nodos del cluster.
2. Cambia `NATS_URL` de las apps al nodo o VIP del cluster.
3. Sube `NATS_STREAM_REPLICAS=3` y `EMAIL_JETSTREAM_REPLICAS=3`.
4. Reinicia API y workers para que reconcilien streams.

## Recursos sugeridos por nodo

- CPU: `2 vCPU`
- RAM: `4 GB`
- Disco: segun `NATS_JETSTREAM_MAX_FILE_STORE`
- `nofile`: `65536`

## Notas

- El cluster de NATS no reemplaza un balanceador TCP si quieres un solo endpoint estable.
- Los certificados TLS del servidor deben incluir el hostname/IP real anunciada por cada nodo.
