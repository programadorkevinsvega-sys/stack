# 📊 Monitoreo de Recursos - MySQL

Documentación específica para monitorear el consumo de recursos del contenedor MySQL.

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats mysql --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats mysql --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect mysql --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats mysql --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Variables de memoria en MySQL
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW VARIABLES WHERE variable_name IN (
  'innodb_buffer_pool_size',
  'key_buffer_size',
  'query_cache_size',
  'max_connections'
);"
```

### Status de memoria en tiempo real
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW STATUS LIKE '%memory%';"
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep mysql
```

### Tamaño del volumen
```bash
docker run --rm -v mysql:/data busybox du -sh /data
```

### Tamaño de bases de datos
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SELECT 
    table_schema,
    ROUND(SUM(data_length + index_length) / 1024 / 1024 / 1024, 2) as 'Size(GB)'
FROM information_schema.TABLES
GROUP BY table_schema
ORDER BY SUM(data_length + index_length) DESC;"
```

### Tamaño de tablas específicas
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SELECT 
    table_schema,
    table_name,
    ROUND(((data_length + index_length) / 1024 / 1024), 2) as 'Size(MB)'
FROM information_schema.TABLES
WHERE table_schema = 'database_name'
ORDER BY (data_length + index_length) DESC;"
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats mysql --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Queries activas (en ejecución)
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW PROCESSLIST;"
```

### Queries lentas (si está habilitado slow query log)
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SELECT 
    query_time,
    lock_time,
    rows_sent,
    rows_examined,
    sql_text
FROM mysql.slow_log
ORDER BY query_time DESC
LIMIT 10;"
```

### Estadísticas de consultas
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW GLOBAL STATUS LIKE 'Questions';"
```

---

## 5. Red (I/O)

### Tráfico de red
```bash
docker stats mysql --format "table {{.Container}}\t{{.NetIO}}"
```

### Conexiones activas
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW STATUS LIKE 'Threads%';"
```

### Máximo de conexiones
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW VARIABLES LIKE 'max_connections';"
```

### Bytes enviados/recibidos
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW GLOBAL STATUS LIKE 'Bytes%';"
```

---

## 6. Salud y Performance

### Estado general del servidor
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SELECT @@version, @@hostname;"
```

### Uptime del servidor
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW STATUS LIKE 'Uptime';"
```

### Logs del contenedor
```bash
docker logs mysql -f --tail 100
```

### Índices no usados
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SELECT 
    object_name,
    count_read,
    count_insert,
    count_update,
    count_delete
FROM performance_schema.table_io_waits_summary_by_index_usage
WHERE count_read = 0 AND object_name != 'INNODB_TRX'
ORDER BY count_insert + count_update + count_delete DESC;"
```

---

## 7. Replicación (si aplica)

### Status de replicación de esclavo
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW SLAVE STATUS\G"
```

### Replication lag
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
SHOW SLAVE STATUS\G" | grep "Seconds_Behind_Master"
```

---

## 8. Optimizaciones y Mantenimiento

### Optimizar tablas (desfragmentación)
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
OPTIMIZE TABLE table_name;"
```

### Analizar tablas (actualizar estadísticas)
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
ANALYZE TABLE table_name;"
```

### Reparar tablas dañadas
```bash
docker exec -it mysql mysql -u root -p$MYSQL_ROOT_PASSWORD -e "
REPAIR TABLE table_name;"
```

---

## 9. Script de Monitoreo Completo

Crear archivo `monitor_mysql.sh`:

```bash
#!/bin/bash
# monitor_mysql.sh - Monitoreo de MySQL

MYSQL_PWD=$MYSQL_ROOT_PASSWORD

echo "======================================"
echo "🐬 MONITOREO MYSQL"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats mysql --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v mysql:/data busybox du -sh /data
echo ""
echo "Bases de datos:"
docker exec mysql mysql -u root -p$MYSQL_PASSWORD -e "
SELECT 
    table_schema,
    ROUND(SUM(data_length + index_length) / 1024 / 1024 / 1024, 2) as 'Size(GB)'
FROM information_schema.TABLES
GROUP BY table_schema ORDER BY SUM(data_length + index_length) DESC;" 2>/dev/null
echo ""

echo "🧠 MEMORIA"
docker exec mysql mysql -u root -p$MYSQL_PASSWORD -e "
SHOW VARIABLES WHERE variable_name IN ('innodb_buffer_pool_size', 'key_buffer_size');" 2>/dev/null
echo ""

echo "🔗 CONEXIONES"
docker exec mysql mysql -u root -p$MYSQL_PASSWORD -e "
SHOW STATUS LIKE 'Threads%';" 2>/dev/null
echo ""

echo "⚡ QUERIES ACTIVAS"
docker exec mysql mysql -u root -p$MYSQL_PASSWORD -e "
SELECT TIME, STATE, INFO FROM information_schema.PROCESSLIST WHERE INFO IS NOT NULL LIMIT 5;" 2>/dev/null
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_mysql.sh
./monitor_mysql.sh
```

---

## 10. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 75%** del límite
- **Conexiones > 80%** del máximo
- **Slow queries > 100ms** frecuentemente

```bash
# Verificar conexiones críticas
docker exec mysql mysql -u root -p$MYSQL_PASSWORD -e "
SHOW STATUS LIKE 'Threads_connected';"
```

---

## 11. Optimizaciones

### Cambiar límite de memoria
```bash
docker update --memory 4g mysql
docker restart mysql
```

### Habilitar slow query log
```bash
docker exec -it mysql mysql -u root -p$MYSQL_PASSWORD -e "
SET GLOBAL slow_query_log = ON;
SET GLOBAL long_query_time = 2;"
```

### Crear índices (mejora performance)
```bash
docker exec -it mysql mysql -u root -p$MYSQL_PASSWORD -e "
ALTER TABLE table_name ADD INDEX idx_name (column_name);"
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats mysql` | Stats en tiempo real |
| `docker logs mysql -f` | Logs |
| `docker exec mysql mysql -V` | Versión |
| `docker volume inspect mysql` | Info del volumen |
| `docker exec mysql mysqldump -u root -ppassword DB > backup.sql` | Backup |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
