# 📊 Monitoreo de Recursos - Redis

Documentación específica para monitorear el consumo de recursos del contenedor Redis.

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats redis --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats redis --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect redis --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats redis --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Información detallada de memoria en Redis
```bash
docker exec -it redis redis-cli INFO memory
```

### Desglose de servidor Redis
```bash
docker exec -it redis redis-cli INFO server
```

### Consumo específico
```bash
docker exec -it redis redis-cli --raw INFO memory | grep -E "used_memory_human|maxmemory_human|used_memory_peak_human"
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep redis
```

### Tamaño del volumen
```bash
docker run --rm -v redis:/data busybox du -sh /data
```

### Tamaño del archivo RDB (dump.rdb)
```bash
docker exec redis ls -lh /data/dump.rdb
```

### Información de persistencia
```bash
docker exec -it redis redis-cli INFO persistence
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats redis --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Comando latency (identifica comandos lentos)
```bash
docker exec -it redis redis-cli LATENCY DOCTOR
```

### Histórico de latencias
```bash
docker exec -it redis redis-cli LATENCY HISTORY
```

### Estadísticas de operaciones
```bash
docker exec -it redis redis-cli INFO stats
```

---

## 5. Red (I/O)

### Tráfico de red
```bash
docker stats redis --format "table {{.Container}}\t{{.NetIO}}"
```

### Clientes conectados
```bash
docker exec -it redis redis-cli INFO clients
```

### Máximo de clientes
```bash
docker exec -it redis redis-cli CONFIG GET maxclients
```

### Estado de conexiones
```bash
docker exec -it redis redis-cli CLIENT LIST
```

---

## 6. Salud y Performance

### Estado general
```bash
docker exec -it redis redis-cli INFO
```

### Versión de Redis
```bash
docker exec -it redis redis-cli INFO server | grep redis_version
```

### Logs del contenedor
```bash
docker logs redis -f --tail 100
```

### Keys por tipo de datos
```bash
docker exec -it redis redis-cli INFO keyspace
```

### Modo replicación (si está configurado)
```bash
docker exec -it redis redis-cli INFO replication
```

---

## 7. Estadísticas Detalladas

### Hit rate del cache
```bash
docker exec -it redis redis-cli INFO stats | grep -E "keyspace_hits|keyspace_misses"
```

### Comandos más ejecutados
```bash
docker exec -it redis redis-cli INFO commandstats
```

### Tamaño de las keys
```bash
docker exec -it redis redis-cli DEBUG OBJECT key_name
```

### Scan de todas las keys (cuidado en producción)
```bash
docker exec -it redis redis-cli --scan --pattern "*" | head -20
```

---

## 8. Monitoreo en Tiempo Real

### Monitor de comandos (verbose)
```bash
docker exec -it redis redis-cli MONITOR
```

### Watch mode (simula watch en bash)
```bash
watch -n 1 'docker exec redis redis-cli INFO stats'
```

---

## 9. Script de Monitoreo Completo

Crear archivo `monitor_redis.sh`:

```bash
#!/bin/bash
# monitor_redis.sh - Monitoreo de Redis

echo "======================================"
echo "🔴 MONITOREO REDIS"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats redis --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v redis:/data busybox du -sh /data
echo ""
echo "Archivo RDB:"
docker exec redis ls -lh /data/dump.rdb 2>/dev/null || echo "No existe dump.rdb"
echo ""

echo "🧠 MEMORIA"
docker exec redis redis-cli INFO memory | grep -E "used_memory_human|maxmemory_human|used_memory_peak_human"
echo ""

echo "🔗 CLIENTES CONECTADOS"
docker exec redis redis-cli INFO clients | grep connected_clients
echo ""

echo "📈 ESTADÍSTICAS"
docker exec redis redis-cli INFO stats | grep -E "total_commands_processed|total_connections_received"
echo ""

echo "🎯 KEYSPACE"
docker exec redis redis-cli INFO keyspace
echo ""

echo "⚡ HIT RATE"
HITS=$(docker exec redis redis-cli INFO stats | grep keyspace_hits | cut -d: -f2)
MISSES=$(docker exec redis redis-cli INFO stats | grep keyspace_misses | cut -d: -f2)
if [ "$((HITS + MISSES))" -gt 0 ]; then
    RATE=$((HITS * 100 / (HITS + MISSES)))
    echo "Cache Hit Rate: ${RATE}%"
fi
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_redis.sh
./monitor_redis.sh
```

---

## 10. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 85%** (Redis es muy sensible)
- **Clientes > 1000** conexiones
- **Hit rate < 90%** (ineficiencia de cache)
- **Latencia > 10ms** (problemas de performance)

```bash
# Verificar hit rate
docker exec redis redis-cli LATENCY LATEST
```

---

## 11. Optimizaciones

### Cambiar límite de memoria
```bash
docker exec -it redis redis-cli CONFIG SET maxmemory 2gb
docker exec redis redis-cli CONFIG REWRITE
```

### Cambiar política de evicción
```bash
docker exec -it redis redis-cli CONFIG SET maxmemory-policy allkeys-lru
```

### Limpiar memoria (flush)
```bash
# Solo la DB actual
docker exec redis redis-cli FLUSHDB

# Todas las DBs (CUIDADO!)
docker exec redis redis-cli FLUSHALL
```

### Verificar configuración actual
```bash
docker exec redis redis-cli CONFIG GET maxmemory
docker exec redis redis-cli CONFIG GET maxmemory-policy
```

---

## 12. Performance Testing

### REDIS-BENCHMARK
```bash
# Test simple
docker exec redis redis-benchmark -t ping -c 50 -n 100000

# Test completo
docker exec redis redis-benchmark -c 100 -n 1000000

# Con payload grande
docker exec redis redis-benchmark -c 50 -n 100000 -d 1024
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats redis` | Stats en tiempo real |
| `docker logs redis -f` | Logs |
| `docker exec redis redis-cli INFO` | Info general |
| `docker exec redis redis-cli CONFIG GET maxmemory` | Ver límite |
| `docker exec redis redis-cli DBSIZE` | Número de keys |
| `docker exec redis redis-cli FLUSHDB` | Limpiar DB |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
