# 📊 Monitoreo de Recursos - PostgreSQL

Documentación específica para monitorear el consumo de recursos del contenedor PostgreSQL.

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats postgres --no-stream
```

**Salida esperada:**
```
CONTAINER ID   NAME       CPU %     MEM USAGE / LIMIT     MEM %     NET I/O           BLOCK I/O
a1b2c3d4e5f6   postgres   8.2%      512.1MiB / 7.5GiB     6.8%      2.3MB / 1.1MB     124MB / 89MB
```

### Monitoreo continuo cada 5 segundos
```bash
watch -n 5 'docker stats postgres --no-stream'
```

---

## 2. Consumo de MEMORIA Específico

### Verificar límite de memoria asignado
```bash
docker inspect postgres --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats postgres --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Dentro del contenedor - procesos PostgreSQL
```bash
docker exec postgres ps aux | grep postgres
```

### Cache y buffers de PostgreSQL
```bash
docker exec -it postgres psql -U postgres -d postgres -c "
SELECT 
    sum(heap_blks_read) as heap_read, 
    sum(heap_blks_hit) as heap_hit, 
    sum(heap_blks_hit) / (sum(heap_blks_hit) + sum(heap_blks_read)) as ratio
FROM pg_statio_user_tables;
"
```

---

## 3. Consumo de DISCO

### Tamaño total del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep postgres
```

### Tamaño del volumen de datos PostgreSQL
```bash
docker run --rm -v postgres:/data busybox du -sh /data
```

### Dentro de PostgreSQL - tamaño de bases de datos
```bash
docker exec -it postgres psql -U postgres -c "
SELECT datname, pg_size_pretty(pg_database_size(datname)) as size
FROM pg_database
ORDER BY pg_database_size(datname) DESC;
"
```

### Tamaño de tablas específicas
```bash
docker exec -it postgres psql -U postgres -d postgres -c "
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as size
FROM pg_tables
WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
"
```

### Crecimiento del WAL (Write-Ahead Logs)
```bash
docker exec postgres du -sh /var/lib/postgresql/data/pg_wal/
```

---

## 4. Consumo de CPU

### CPU actual del contenedor
```bash
docker stats postgres --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Queries lentas (afectan CPU)
```bash
docker exec -it postgres psql -U postgres -c "
SELECT 
    query,
    mean_exec_time,
    calls,
    max_exec_time
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 10;
"
```

### Procesos activos en PostgreSQL
```bash
docker exec -it postgres psql -U postgres -c "
SELECT 
    pid,
    usename,
    application_name,
    client_hostname,
    state,
    query
FROM pg_stat_activity
WHERE state != 'idle'
ORDER BY query_start DESC;
"
```

---

## 5. Red (I/O)

### Tráfico de red del contenedor
```bash
docker stats postgres --format "table {{.Container}}\t{{.NetIO}}"
```

### Conexiones activas
```bash
docker exec -it postgres psql -U postgres -c "
SELECT count(*) as conexiones_activas
FROM pg_stat_activity;
"
```

### Máximo de conexiones permitidas
```bash
docker exec -it postgres psql -U postgres -c "SHOW max_connections;"
```

---

## 6. Salud y Performance

### Estado general del contenedor
```bash
docker inspect postgres --format='{{json .State}}' | jq
```

### Logs del contenedor
```bash
docker logs postgres -f --tail 100
```

### Vacuum y Autovacuum (limpieza de basura)
```bash
docker exec -it postgres psql -U postgres -c "
SELECT 
    schemaname,
    relname,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
FROM pg_stat_user_tables
ORDER BY last_autovacuum DESC;
"
```

### Índices no usados (por optimizar)
```bash
docker exec -it postgres psql -U postgres -c "
SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan as scans
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;
"
```

---

## 7. Benchmarking y Testing

### PGBENCH - Test de carga
```bash
# Preparar base de datos de test
docker exec postgres pgbench -i -s 100 bench_db

# Ejecutar test (30 segundos, 10 clientes)
docker exec postgres pgbench -c 10 -j 4 -T 30 bench_db
```

### Ver config actual
```bash
docker exec -it postgres psql -U postgres -c "
SELECT name, setting, unit 
FROM pg_settings 
WHERE name LIKE '%buffer%' OR name LIKE '%cache%' OR name LIKE '%work%';
"
```

---

## 8. Script de Monitoreo Completo

Crear archivo `monitor_postgres.sh`:

```bash
#!/bin/bash
# monitor_postgres.sh - Monitoreo de PostgreSQL

echo "======================================"
echo "🐘 MONITOREO POSTGRESQL"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats postgres --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v postgres:/data busybox du -sh /data
echo ""
echo "Bases de datos:"
docker exec postgres psql -U postgres -c "
SELECT datname, pg_size_pretty(pg_database_size(datname)) as size
FROM pg_database ORDER BY pg_database_size(datname) DESC;" 2>/dev/null
echo ""

echo "🧠 MEMORIA ASIGNADA"
docker inspect postgres --format='{{.HostConfig.Memory}}' | awk '{printf "Límite: %.2f GiB\n", $1/1073741824}'
echo ""

echo "🔗 CONEXIONES ACTIVAS"
docker exec postgres psql -U postgres -c "SELECT count(*) as activas FROM pg_stat_activity;" 2>/dev/null
echo ""

echo "⚡ TOP QUERIES (más lentas)"
docker exec postgres psql -U postgres -c "
SELECT query, round(mean_exec_time::numeric,2) as ms
FROM pg_stat_statements ORDER BY mean_exec_time DESC LIMIT 5;" 2>/dev/null
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_postgres.sh
./monitor_postgres.sh
```

---

## 9. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 75%** del límite
- **Conexiones > 80%** del máximo
- **Tamaño WAL > 1GB** sin checkpoint

```bash
# Conexiones críticas
docker exec postgres psql -U postgres -c "
SELECT count(*) as total
FROM pg_stat_activity
WHERE state != 'idle'
" | tail -1 | awk '{if($1 > 80) print "⚠️  ALERTA: Conexiones altas: "$1}'
```

---

## 10. Optimizaciones

### Cambiar límite de memoria
```bash
docker update --memory 4g postgres
docker restart postgres
```

### Ver configuración de performance
```bash
docker exec -it postgres psql -U postgres -c "
SELECT name, setting 
FROM pg_settings 
WHERE name IN ('shared_buffers', 'effective_cache_size', 'work_mem', 'maintenance_work_mem');"
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats postgres` | Stats en tiempo real |
| `docker logs postgres -f` | Logs |
| `docker exec postgres psql -U postgres -c "SELECT version();"` | Versión |
| `docker volume inspect postgres` | Info del volumen |
| `docker exec postgres pg_dump -U postgres DB > backup.sql` | Backup |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
