# 📊 Monitoreo de Recursos - n8n (VPS Workers)

Documentación específica para monitorear el consumo de recursos del stack n8n en VPS con workers distribuidos.

---

## 1. Arquitectura Distribuida

```
┌─────────────────┐
│  VPS Remoto     │
│  ┌───────────┐  │
│  │ n8n Core  │  │
│  │ (Main)    │  │
│  └───────────┘  │
│  ┌───────────┐  │
│  │ PostgreSQL│  │ (Base de datos compartida)
│  └───────────┘  │
└─────────────────┘
        ↕ (Shared: RabbitMQ, PostgreSQL)
┌─────────────────────────────────────────┐
│  PC Local + Worker(s)                   │
│  ┌───────────┐    ┌────────────────┐   │
│  │ Worker 1  │    │ Worker N       │   │
│  └───────────┘    └────────────────┘   │
└─────────────────────────────────────────┘
```

---

## 2. Monitoreo por Componente - VPS

### n8n Core (Main)
```bash
docker stats n8n --no-stream
```

### PostgreSQL en VPS
```bash
docker stats postgres --no-stream
```

### RabbitMQ (Broker compartido)
```bash
docker stats rabbitmq --no-stream
```

---

## 3. Monitoreo por Componente - PC Local/Workers

### Worker 1, 2, N...
```bash
docker stats n8n-worker-1 n8n-worker-2 --no-stream
```

### Estado de todos los workers
```bash
docker ps --filter "ancestor=n8n:latest" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
```

### Monitoreo continuo de workers
```bash
watch -n 5 'docker stats n8n-worker-* --no-stream'
```

---

## 4. Health Check del Cluster

### n8n Core disponible
```bash
curl -s http://VPS_IP:5678/health | jq '.status'
```

### PostgreSQL responde
```bash
docker exec postgres psql -U n8n -d n8n -c "SELECT 1;"
```

### RabbitMQ management
```bash
curl -s http://localhost:15672/api/overview \
  -u guest:guest | jq '.object_totals'
```

### Workers conectados (via RabbitMQ)
```bash
curl -s http://localhost:15672/api/connections \
  -u guest:guest | jq 'length'
```

---

## 5. Consumo de Recursos VPS

### Stats completos del VPS
```bash
docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}"
```

### Uso de disco en VPS
```bash
docker system df
docker system df -v | grep -E "n8n|postgres|rabbitmq"
```

### Espacio en /var/lib/docker
```bash
df -h /var/lib/docker/
```

---

## 6. Consumo de Recursos PC Local

### Stats de todos los workers en PC
```bash
docker stats n8n-worker-* --no-stream
```

### Suma de memoria de workers
```bash
docker stats n8n-worker-* --format "{{.MemUsage}}" --no-stream | \
  awk '{gsub(/MiB|GiB/, ""); sum+=$1} END {print "Total: " sum " MB"}'
```

### CPU total de workers
```bash
docker stats n8n-worker-* --format "{{.CPUPerc}}" --no-stream | \
  awk -F% '{sum+=$1} END {print "Total CPU: " sum "%"}'
```

---

## 7. Base de Datos Compartida (PostgreSQL en VPS)

### Tamaño total BD
```bash
docker run --rm -v postgres:/data busybox du -sh /data
```

### Tablas más grandes
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.' || tablename)) as size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size('public.' || tablename) DESC
LIMIT 10;"
```

### Número de workflows
```bash
docker exec postgres psql -U n8n -d n8n -c "SELECT count(*) as workflows FROM workflow;"
```

### Número de executions
```bash
docker exec postgres psql -U n8n -d n8n -c "SELECT count(*) as total_executions FROM execution;"
```

### Executions pendientes
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT count(*) as executions_en_progreso 
FROM execution 
WHERE stoppedAt IS NULL;"
```

### Conexiones activas
```bash
docker exec postgres psql -U n8n -d n8n -c "SELECT count(*) FROM pg_stat_activity WHERE state != 'idle';"
```

---

## 8. RabbitMQ (Message Broker)

### Stats del contenedor
```bash
docker stats rabbitmq --no-stream
```

### Conexiones desde workers
```bash
curl -s http://localhost:15672/api/connections \
  -u $RABBITMQ_DEFAULT_USER:$RABBITMQ_DEFAULT_PASS | jq '.[] | {host, peer_host, state}'
```

### Queues en RabbitMQ
```bash
curl -s http://localhost:15672/api/queues \
  -u $RABBITMQ_DEFAULT_USER:$RABBITMQ_DEFAULT_PASS | jq '.[] | {name, messages, consumers}'
```

### Mensajes en progress
```bash
curl -s http://localhost:15672/api/overview \
  -u $RABBITMQ_DEFAULT_USER:$RABBITMQ_DEFAULT_PASS | jq '.queue_totals'
```

---

## 9. Script Monitoreo Distribuido

Crear archivo `monitor_distributed.sh`:

```bash
#!/bin/bash
# monitor_distributed.sh - Monitoreo de stack distribuido n8n

RABBIT_USER=${RABBITMQ_DEFAULT_USER:-guest}
RABBIT_PASS=${RABBITMQ_DEFAULT_PASS:-guest}
VPS_IP=${VPS_IP:-"localhost"}

echo "======================================"
echo "⚙️  MONITOREO N8N DISTRIBUIDO"
echo "$(date)"
echo "======================================"
echo ""

echo "🖥️  VPS REMOTO"
echo "IP: $VPS_IP"
echo "SSH (opcional): ssh user@$VPS_IP"
echo ""

echo "📊 STATS VPS"
docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}" | head -10
echo ""

echo "💾 DISCO (VPS)"
docker system df | head -5
echo ""

echo "🔴 POSTGRESQL (Compartida)"
echo "Tamaño:"
docker run --rm -v postgres:/data busybox du -sh /data 2>/dev/null
echo "Conexiones activas:"
docker exec postgres psql -U n8n -d n8n -c "SELECT count(*) FROM pg_stat_activity WHERE state != 'idle';" 2>/dev/null
echo ""

echo "🐰 RABBITMQ (Broker)"
echo "Conexiones:"
curl -s http://localhost:15672/api/connections \
  -u $RABBIT_USER:$RABBIT_PASS 2>/dev/null | jq 'length'
echo "Queues:"
curl -s http://localhost:15672/api/queues \
  -u $RABBIT_USER:$RABBIT_PASS 2>/dev/null | jq '.[] | {name, messages, consumers}'
echo ""

echo "⚙️  N8N CORE (VPS)"
curl -s http://$VPS_IP:5678/health 2>/dev/null | jq '{status, uptime}' || echo "No responde"
echo ""

echo "👷 WORKERS (Local)"
WORKERS=$(docker ps --filter "ancestor=n8n:latest" --format "{{.Names}}" | grep worker)
if [ ! -z "$WORKERS" ]; then
  echo "Trabajadores activos: $(echo "$WORKERS" | wc -l)"
  docker stats --no-stream $(echo "$WORKERS" | tr '\n' ' ')
else
  echo "No hay workers activos"
fi
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_distributed.sh
export VPS_IP="192.168.1.100"
./monitor_distributed.sh
```

---

## 10. Validación de Conectividad

### ¿VPS y PC pueden conectar?
```bash
# Desde PC hacia VPS
curl -s http://$VPS_IP:5678/health

# Verificar RabbitMQ conecta
curl -s http://localhost:15672/api/connections -u guest:guest | jq '.[] | .host'
```

### ¿PostgreSQL accesible desde ambos?
```bash
# VPS
docker exec postgres psql -U n8n -d n8n -c "SELECT NOW();"

# PC (si PostgreSQL remota)
psql -h $VPS_IP -U n8n -d n8n -c "SELECT NOW();"
```

---

## 11. Performance Distribuido

### Latencia RabbitMQ
```bash
curl -s http://localhost:15672/api/overview \
  -u $RABBIT_USER:$RABBIT_PASS | jq '.queue_totals | {messages_details}'
```

### Queries lentas en PostgreSQL
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT 
    query_time,
    query
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 5;"
```

### Balanceo de carga entre workers
```bash
# Ver qué worker procesa qué
curl -s http://localhost:15672/api/channels \
  -u $RABBIT_USER:$RABBIT_PASS | jq '.[] | {name, connection_details}'
```

---

## 12. Alertas Recomendadas

Monitorizar si:
- **Conexión RabbitMQ perdida** (0 connections)
- **PostgreSQL no responde**
- **N8N Core no responde**
- **Workers sin conectar** (0 workers)
- **Queues acumulando mensajes** (sin procesar)
- **CPU/Memoria > 85%** en VPS
- **CPU/Memoria > 85%** en workers (PC)
- **Disk space < 20%** en ambas máquinas

```bash
# Quick alerts
echo "🚨 Verificando alertas..."
[ $(curl -s http://localhost:15672/api/connections -u guest:guest | jq 'length') -lt 1 ] && echo "⚠️  RabbitMQ sin workers!"
[ $(docker ps -q --filter "ancestor=n8n:latest" | wc -l) -eq 0 ] && echo "⚠️  N8N Core no corriendo!"
```

---

## 13. Sincronización y Logs

### Logs de n8n Core (VPS)
```bash
ssh user@$VPS_IP "docker logs n8n -f --tail 50"
```

### Logs de un worker (PC)
```bash
docker logs n8n-worker-1 -f --tail 50
```

### Ejecuciones cruzadas
```bash
# Ver workflow inicializado en VPS
docker exec postgres psql -U n8n -d n8n -c "
SELECT id, name, active FROM workflow ORDER BY createdAt DESC LIMIT 5;"

# Ver executions desde workers
docker exec postgres psql -U n8n -d n8n -c "
SELECT id, workflowId, status, startedAt FROM execution ORDER BY createdAt DESC LIMIT 5;"
```

---

## 14. Optimizaciones Distribuidas

### Aumentar workers
```bash
# En PC, agregar docker-compose con múltiples workers
docker-compose -f docker-compose.vps.yml up -d --scale n8n-worker=3
```

### Aumentar throughput RabbitMQ
```bash
# En VPS/database/rabbitmq/docker-compose.rabbitmq.yml
# Aumentar consumers y prefetch
```

### Pool de conexiones PostgreSQL
```bash
docker exec postgres psql -U n8n -d n8n -c "
ALTER SYSTEM SET max_connections = 200;"
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `./monitor_distributed.sh` | Reporte completo |
| `curl http://$VPS_IP:5678/health` | Health del core |
| `docker stats n8n-worker-*` | Stats workers |
| `curl http://localhost:15672/api/overview -u guest:guest` | Info RabbitMQ |
| `ssh user@$VPS_IP "docker logs n8n"` | Logs core (remoto) |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
