# 📊 Monitoreo de Recursos - MinIO

Documentación específica para monitorear el consumo de recursos del contenedor MinIO (almacenamiento S3).

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats minio --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats minio --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect minio --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats minio --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Procesos de MinIO
```bash
docker exec minio ps aux | grep minio
```

### Memoria dentro del contenedor
```bash
docker exec minio top -b -n 1 | head -15
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep minio
```

### Tamaño del volumen
```bash
docker run --rm -v minio:/data busybox du -sh /data
```

### Desglose por carpeta
```bash
docker exec minio du -sh /data/* | sort -hr
```

### Espacio disponible
```bash
docker exec minio df -h /data
```

### Info de salud (via API)
```bash
curl -s http://localhost:9000/minio/health/live | jq
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats minio --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Procesos de CPU alto
```bash
docker exec minio top -b -n 1 | sort -k9 -nr | head -10
```

---

## 5. Buckets y Objetos

### Listar buckets (requiere mc - MinIO Client)
```bash
# Primero configurar cliente MinIO
docker exec -e MINIO_ROOT_USER=${MINIO_ROOT_USER} \
  -e MINIO_ROOT_PASSWORD=${MINIO_ROOT_PASSWORD} \
  minio mc alias set minio http://localhost:9000 $MINIO_ROOT_USER $MINIO_ROOT_PASSWORD

# Listar buckets
docker exec minio mc ls minio/
```

### Tamaño de buckets
```bash
docker exec minio mc du minio/ --recursive
```

### Info de bucket específico
```bash
docker exec minio mc ls minio/bucket-name/
```

### Número de objetos por bucket
```bash
docker exec minio bash -c '
  find /data -type f | wc -l'
```

---

## 6. Admin y Cluster

### Info del servidor MinIO
```bash
curl -s http://localhost:9000/minio/admin-api/v1/admin-info \
  -H "Authorization: Bearer $TOKEN"
```

### Estado del cluster (si está configurado)
```bash
docker exec minio mc admin info minio
```

### Replicación status
```bash
docker exec minio mc stat minio/bucket-name/object-name --json | jq '.replication'
```

---

## 7. Metrics y Monitoring

### Prometheus metrics
```bash
curl -s http://localhost:9000/minio/v2/metrics/cluster | head -50
```

### Health check completo
```bash
docker exec minio curl -s http://localhost:9000/minio/health/live
docker exec minio curl -s http://localhost:9000/minio/health/ready
```

### Estadísticas en tiempo real
```bash
watch -n 2 'curl -s http://localhost:9000/minio/v2/metrics/cluster | grep "minio_" | head -10'
```

---

## 8. Network (I/O)

### Tráfico de red
```bash
docker stats minio --format "table {{.Container}}\t{{.NetIO}}"
```

### Conexiones activas
```bash
docker exec minio netstat -tuln | grep LISTEN
```

---

## 9. Acceso MinIO Console

### URL de consola web
```
http://localhost:9001

Usuario:    ${MINIO_ROOT_USER}
Contraseña: ${MINIO_ROOT_PASSWORD}
```

**Proporciona:**
- Navegación de buckets
- Gestión de objetos
- Usuarios y permisos
- Configuración del servidor
- Monitoreo en tiempo real

---

## 10. Script de Monitoreo Completo

Crear archivo `monitor_minio.sh`:

```bash
#!/bin/bash
# monitor_minio.sh - Monitoreo de MinIO

echo "======================================"
echo "🪣 MONITOREO MINIO"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats minio --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v minio:/data busybox du -sh /data
echo ""
echo "Espacio disponible:"
docker exec minio df -h /data | tail -1
echo ""
echo "Primeras carpetas (por tamaño):"
docker exec minio du -sh /data/* 2>/dev/null | sort -hr | head -5
echo ""

echo "🧠 MEMORIA"
docker exec minio ps aux | grep -v grep | grep minio | awk '{printf "Memoria: %s\n", $6}'
echo ""

echo "🌐 PUERTOS ESCUCHANDO"
docker exec minio netstat -tuln | grep LISTEN | grep 9000
echo ""

echo "📦 INFO DEL SERVIDOR"
curl -s http://localhost:9000/minio/health/live | jq 2>/dev/null || echo "MinIO no responde"
echo ""

echo "💻 TOP PROCESOS"
docker exec minio top -b -n 1 | tail -n +8 | head -5
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_minio.sh
./monitor_minio.sh
```

---

## 11. MinIO Client (mc) - Comandos Útiles

### Configurar cliente
```bash
docker exec minio mc alias set local http://localhost:9000 minioadmin minioadmin
```

### Listar contenido
```bash
docker exec minio mc ls local/
```

### Copy entre buckets
```bash
docker exec minio mc cp local/origen/objeto local/destino/
```

### Remove recursivo
```bash
docker exec minio mc rm local/bucket/ --recursive --force
```

### Stat de objeto
```bash
docker exec minio mc stat local/bucket/objeto
```

---

## 12. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 75%** del límite
- **Disk space < 20%**
- **Health check falla** (live/ready)
- **Latencia I/O > 100ms**

---

## 13. Optimizaciones

### Aumentar memoria
```bash
docker update --memory 4g minio
docker restart minio
```

### Limpiar buckets no usados
```bash
# Listar todos
docker exec minio mc ls local/

# Remover bucket vacío
docker exec minio mc rb local/bucket-name --force
```

### Backup de bucket
```bash
docker exec minio mc mirror local/bucket-name /tmp/backup/
```

### Restaurar bucket
```bash
docker exec minio mc mirror /tmp/backup/ local/bucket-name/
```

---

## 14. RAID/Erasure Coding

MinIO soporta Erasure Coding automático (tipo RAID):

```bash
# Ver configuración actual
docker exec minio mc admin config get local

# Configurar parámetros
docker exec minio mc admin config set local \
  erasure standard_4 \
  --api=S3v4
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats minio` | Stats en tiempo real |
| `docker logs minio -f` | Logs |
| `http://localhost:9001` | Console MinIO |
| `docker exec minio mc ls` | Listar buckets |
| `docker exec minio mc du` | Tamaño de buckets |
| `docker exec minio df -h /data` | Espacio disponible |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
