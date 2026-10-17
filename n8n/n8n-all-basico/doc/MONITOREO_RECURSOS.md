# 📊 Monitoreo de Recursos - n8n (All-in-One Básico)

Documentación específica para monitorear el consumo de recursos del stack n8n all-in-one básico.

---

## 1. Stack Completo - Monitoreo General

### Ver todos los contenedores
```bash
docker stats
```

### Ver solo n8n y sus dependencias
```bash
docker ps -a | grep -E "n8n|postgres|redis"
```

### Monitoreo del stack
```bash
watch -n 5 'docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"'
```

---

## 2. Consumo Agregado del Stack

### Stats de múltiples contenedores
```bash
docker stats --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}" --no-stream | grep -E "postgres|redis|n8n"
```

### Espacio total usado
```bash
docker system df
```

### Espacio detallado
```bash
docker system df -v | grep -E "postgres|redis|n8n"
```

---

## 3. n8n (Contenedor Principal)

### Stats de n8n
```bash
docker stats n8n --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats n8n --no-stream'
```

### Memoria
```bash
docker stats n8n --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### CPU
```bash
docker stats n8n --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Tamaño
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep n8n
```

### Logs
```bash
docker logs n8n -f --tail 100
```

---

## 4. PostgreSQL (Base de Datos)

### Stats de PostgreSQL
```bash
docker stats postgres --no-stream
```

### Tamaño de BD
```bash
docker run --rm -v postgres:/data busybox du -sh /data
```

### Bases de datos
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT datname, pg_size_pretty(pg_database_size(datname))
FROM pg_database WHERE datistemplate = false;"
```

### Tablas principales
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.' || tablename)) as size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size('public.' || tablename) DESC;"
```

### Conexiones
```bash
docker exec postgres psql -U n8n -d n8n -c "SELECT count(*) FROM pg_stat_activity;"
```

---

## 5. Redis (Cache/Queue)

### Stats de Redis
```bash
docker stats redis --no-stream
```

### Tamaño del volumen
```bash
docker run --rm -v redis:/data busybox du -sh /data
```

### Información de Redis
```bash
docker exec redis redis-cli INFO memory
```

### Keys almacenadas
```bash
docker exec redis redis-cli DBSIZE
```

### Hit rate
```bash
docker exec redis redis-cli INFO stats | grep -E "keyspace"
```

---

## 6. Network (I/O del Stack)

### Tráfico de red por contenedor
```bash
docker stats --format "table {{.Container}}\t{{.NetIO}}" --no-stream | grep -E "postgres|redis|n8n"
```

### Puertos en uso
```bash
netstat -tuln | grep -E "5432|6379|5678"
```

---

## 7. Script Monitoreo Completo del Stack

Crear archivo `monitor_all.sh`:

```bash
#!/bin/bash
# monitor_all.sh - Monitoreo del stack n8n all-in-one

echo "======================================"
echo "⚙️  MONITOREO STACK N8N ALL-IN-ONE"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 ESTADO DE CONTENEDORES"
docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" | grep -E "n8n|postgres|redis|NAMES"
echo ""

echo "💾 CONSUMO DE DISCO TOTAL"
docker system df
echo ""

echo "📈 RAM (por contenedor)"
echo "Contenedor | Uso Actual | Límite | %"
docker stats --no-stream --format "{{.Container}}\t{{.MemUsage}}" 2>/dev/null | grep -E "n8n|postgres|redis"
echo ""

echo "⚡ CPU (por contenedor)"
docker stats --no-stream --format "{{.Container}}\t{{.CPUPerc}}" | grep -E "n8n|postgres|redis"
echo ""

echo "🧠 POSTGRES"
echo "Tamaño BD:"
docker run --rm -v postgres:/data busybox du -sh /data 2>/dev/null
echo "Conexiones:"
docker exec postgres psql -U n8n -d n8n -c "SELECT count(*) FROM pg_stat_activity;" 2>/dev/null
echo ""

echo "🔴 REDIS"
echo "Tamaño volumen:"
docker run --rm -v redis:/data busybox du -sh /data 2>/dev/null
echo "Keys:"
docker exec redis redis-cli DBSIZE 2>/dev/null
echo ""

echo "⚙️  N8N"
echo "Status:"
curl -s http://localhost:5678/health 2>/dev/null | jq '.status' || echo "No responde"
echo ""

echo "🌐 ESPACIO DOCKER"
df -h /var/lib/docker/ 2>/dev/null | tail -1 || echo "N/A"
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_all.sh
./monitor_all.sh
```

---

## 8. Health Checks del Stack

### n8n disponible
```bash
curl -s http://localhost:5678/health | jq '.status'
```

### PostgreSQL responde
```bash
docker exec postgres psql -U n8n -d n8n -c "SELECT 1;"
```

### Redis responde
```bash
docker exec redis redis-cli PING
```

### Todo el stack OK
```bash
bash -c '
echo "n8n: $(curl -s http://localhost:5678/health | jq -r .status)"
echo "PostgreSQL: $(docker exec postgres psql -U n8n -d n8n -c \"SELECT 1;\" 2>&1 | grep -q 1 && echo OK || echo FAIL)"
echo "Redis: $(docker exec redis redis-cli PING 2>&1)"
'
```

---

## 9. Performance del Stack

### Top comandos en PostgreSQL
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT query, calls, mean_exec_time
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 5;"
```

### Workflows en ejecución
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT count(*) as ejecutando FROM execution
WHERE stoppedAt IS NULL;"
```

### Ejecutar benchmark Redis
```bash
docker exec redis redis-benchmark -n 10000
```

---

## 10. Limpieza y Optimización

### Limpiar volúmenes no usados
```bash
docker volume prune -f
```

### Compactar BD PostgreSQL
```bash
docker exec postgres psql -U n8n -d n8n -c "VACUUM ANALYZE;"
```

### Limpiar cache Redis
```bash
docker exec redis redis-cli FLUSHDB
```

### Reiniciar todo el stack
```bash
docker-compose -f docker-compose-basico.yml restart
```

---

## 11. Alertas Recomendadas

Monitorizar si:
- **N8N no responde** (health endpoint falla)
- **PostgreSQL sin conexión** (psql falla)
- **Redis PING falla**
- **CPU del stack > 80%** sostenido
- **Memoria total > 85%** del límite
- **Disk space < 20%** en /var/lib/docker
- **Múltiples execuciones atoradas** sin completar

```bash
# Quick alert check
docker stats --no-stream | awk 'NR>1 {gsub(/%/,"",$3); if($3+0 > 80) print "⚠️  CPU ALTA: " $1}'
```

---

## 12. Configuración Recomendada

### Límites de recursos en docker-compose.yml
```yaml
services:
  n8n:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 2G
        reservations:
          cpus: '1'
          memory: 1G
  
  postgres:
    deploy:
      resources:
        limits:
          cpus: '1'
          memory: 2G
  
  redis:
    deploy:
      resources:
        limits:
          cpus: '0.5'
          memory: 512M
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats` | Todo en tiempo real |
| `docker system df` | Uso total disco |
| `./monitor_all.sh` | Reporte completo |
| `curl http://localhost:5678/health` | Health n8n |
| `docker-compose -f docker-compose-basico.yml ps` | Estado servicios |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
