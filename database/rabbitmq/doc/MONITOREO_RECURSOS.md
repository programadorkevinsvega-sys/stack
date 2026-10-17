# 📊 Monitoreo de Recursos - RabbitMQ

Documentación específica para monitorear el consumo de recursos del contenedor RabbitMQ.

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats rabbitmq --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats rabbitmq --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect rabbitmq --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats rabbitmq --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Memoria por nodo (via Management API)
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/nodes | jq '.[] | {name, memory_used, memory_limit}'
```

### Información detallada de memoria
```bash
docker exec rabbitmq rabbitmqctl status | grep -i memory
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep rabbitmq
```

### Tamaño del volumen
```bash
docker run --rm -v rabbitmq:/data busybox du -sh /data
```

### Espacio disponible en disco
```bash
docker exec rabbitmq df -h /var/lib/rabbitmq
```

### Información de disco (via API)
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/nodes | jq '.[] | {name, disk_free, disk_limit}'
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats rabbitmq --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Procesos de RabbitMQ
```bash
docker exec rabbitmq ps aux | grep rabbitmq
```

### Con top dentro del contenedor
```bash
docker exec rabbitmq top -b -n 1 | head -20
```

---

## 5. Conexiones y Canales

### Conexiones activas (via API)
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/connections | jq 'length'
```

### Detalles de conexiones
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/connections | jq '.[] | {name, host, port, state}'
```

### Número de canales
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/channels | jq 'length'
```

---

## 6. Queues (Colas)

### Listar todas las queues
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/queues | jq '.[] | {name, messages, messages_ready, messages_unacked}'
```

### Queue específica
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/queues/%2F/queue_name | jq '{
    name: .name,
    messages: .messages,
    messages_ready: .messages_ready,
    messages_unacked: .messages_unacked,
    consumers: .consumers
}'
```

### Estadísticas de mensajes
```bash
docker exec rabbitmq rabbitmqctl eval "
{ok, Conn} = amqp_connection:start(#amqp_params_network{}),
Channel = amqp_connection:open_channel(Conn),
{#'queue.declare_ok'{message_count=MessageCount}, _} = amqp_channel:call(Channel, 
  #'queue.declare'{queue = <<"queue_name">>, passive = true}),
io:format('Messages: ~p~n', [MessageCount])."
```

---

## 7. Red (I/O y Tráfico)

### Tráfico de red
```bash
docker stats rabbitmq --format "table {{.Container}}\t{{.NetIO}}"
```

### Mensajes enviados/recibidos (via API)
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/overview | jq '{
    messages_total: .queue_totals.messages,
    messages_ready: .queue_totals.messages_ready,
    messages_unacked: .queue_totals.messages_unacked,
    publish_rate: .queue_totals.messages_details.avg_rate
}'
```

---

## 8. Health Check y Performance

### Estado del sistema
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/overview | jq '{
    cluster_name: .cluster_name,
    erlang_version: .erlang_version,
    rabbitmq_version: .rabbitmq_version,
    uptime: .uptime,
    statistics_level: .statistics_level
}'
```

### Health check del nodo
```bash
docker exec rabbitmq rabbitmqctl status
```

### Logs del contenedor
```bash
docker logs rabbitmq -f --tail 100
```

### Alarms activos
```bash
curl -s -u ${RABBITMQ_DEFAULT_USER:-guest}:${RABBITMQ_DEFAULT_PASS:-guest} \
  http://localhost:15672/api/alarms | jq
```

---

## 9. Script de Monitoreo Completo

Crear archivo `monitor_rabbitmq.sh`:

```bash
#!/bin/bash
# monitor_rabbitmq.sh - Monitoreo de RabbitMQ

USER=${RABBITMQ_DEFAULT_USER:-guest}
PASS=${RABBITMQ_DEFAULT_PASS:-guest}

echo "======================================"
echo "🐰 MONITOREO RABBITMQ"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats rabbitmq --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v rabbitmq:/data busybox du -sh /data
echo "Espacio disponible:"
docker exec rabbitmq df -h /var/lib/rabbitmq | tail -1
echo ""

echo "🧠 MEMORIA"
curl -s -u $USER:$PASS \
  http://localhost:15672/api/nodes | jq '.[] | {
    name,
    memory_used_human: (.memory_used / 1024 / 1024 | tostring + " MB"),
    memory_limit_human: (.memory_limit / 1024 / 1024 | tostring + " MB")
  }' 2>/dev/null
echo ""

echo "🔗 CONEXIONES Y CANALES"
CONNECTIONS=$(curl -s -u $USER:$PASS http://localhost:15672/api/connections | jq 'length')
CHANNELS=$(curl -s -u $USER:$PASS http://localhost:15672/api/channels | jq 'length')
echo "Conexiones: $CONNECTIONS"
echo "Canales: $CHANNELS"
echo ""

echo "📦 INFORMACIÓN DE QUEUES"
curl -s -u $USER:$PASS \
  http://localhost:15672/api/queues | jq '.[] | {
    name: .name,
    messages: .messages,
    messages_ready: .messages_ready,
    consumers: .consumers
  }' 2>/dev/null | head -20
echo ""

echo "📊 OVERVIEW"
curl -s -u $USER:$PASS \
  http://localhost:15672/api/overview | jq '{
    cluster_name,
    rabbitmq_version,
    uptime,
    object_totals: {
        connections: .object_totals.connections,
        channels: .object_totals.channels,
        queues: .object_totals.queues,
        consumers: .object_totals.consumers
    }
  }' 2>/dev/null
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_rabbitmq.sh
./monitor_rabbitmq.sh
```

---

## 10. Management UI Visual

### URL para interfaz web
```
http://localhost:15672

Usuario:   ${RABBITMQ_DEFAULT_USER:-KevinsAdmin}
Password:  ${RABBITMQ_DEFAULT_PASS:-Kevins1612522026}
```

La UI proporciona:
- **Gráficos en tiempo real** de mensajes
- **Información de nodes**
- **Detalles de conexiones y canales**
- **Cola por cola**
- **Usuarios y permisos**

---

## 11. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 80%** del límite
- **Queues sin descargar** (acumulación de mensajes)
- **Alarms activos** (ver via `/api/alarms`)
- **Disk space < 20%**

```bash
# Verificar alarms
curl -s -u guest:guest http://localhost:15672/api/alarms | jq
```

---

## 12. Optimizaciones

### Aumentar límite de memoria
```bash
docker update --memory 4g rabbitmq
docker restart rabbitmq
```

### Cambiar variables de entorno
```bash
docker exec rabbitmq rabbitmqctl set_vm_memory_high_watermark 0.6
```

### Limpiar queues antiguas
```bash
docker exec rabbitmq rabbitmqctl list_queues name messages | grep "[0-9]" | awk '$2 > 1000000 {print $1}' | while read queue; do
  docker exec rabbitmq rabbitmqctl purge_queue "$queue"
done
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats rabbitmq` | Stats en tiempo real |
| `docker logs rabbitmq -f` | Logs |
| `docker exec rabbitmq rabbitmqctl status` | Status del nodo |
| `docker exec rabbitmq rabbitmqctl list_queues` | Listar queues |
| `docker exec rabbitmq rabbitmqctl list_connections` | Conexiones |
| `http://localhost:15672` | Management UI |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
