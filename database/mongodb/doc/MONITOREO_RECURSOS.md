# 📊 Monitoreo de Recursos - MongoDB

Documentación específica para monitorear el consumo de recursos del contenedor MongoDB.

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats mongodb --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats mongodb --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect mongodb --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741832 " GiB"}'
```

### Consumo actual
```bash
docker stats mongodb --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Estadísticas de memoria dentro de MongoDB
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus().mem"
```

### WiredTiger Cache (motor de almacenamiento)
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus().wiredTiger.cache"
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep mongodb
```

### Tamaño del volumen
```bash
docker run --rm -v mongodb:/data busybox du -sh /data
```

### Tamaño de bases de datos
```bash
docker exec -it mongodb mongosh --eval "
db.adminCommand({listDatabases: 1, nameOnly: true}).databases.forEach(db => {
    const size = db.sizeOnDisk || 0;
    print(db.name + ': ' + (size/1024/1024/1024).toFixed(2) + ' GB');
})"
```

### Tamaño de colecciones específicas
```bash
docker exec -it mongodb mongosh --eval "
use database_name;
db.getCollectionNames().forEach(coll => {
    const stats = db.getCollection(coll).stats();
    print(coll + ': ' + (stats.size/1024/1024).toFixed(2) + ' MB');
})"
```

### Estadísticas de almacenamiento
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus().opcounters"
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats mongodb --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Operaciones por segundo
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus().opcountersRepl"
```

### Queries activas (en ejecución)
```bash
docker exec -it mongodb mongosh --eval "
db.currentOp(true)" | head -50
```

### Queries lentas (profiling)
```bash
docker exec -it mongodb mongosh --eval "
db.system.profile.find().limit(10).sort({ts: -1}).pretty()" 
```

---

## 5. Red (I/O)

### Tráfico de red
```bash
docker stats mongodb --format "table {{.Container}}\t{{.NetIO}}"
```

### Conexiones activas
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus().connections"
```

### Máximo de conexiones
```bash
docker exec -it mongodb mongosh --eval "
db.adminCommand({getParameter: 1, maxIncomingConnections: 1})"
```

---

## 6. Salud y Performance

### Estado general
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus() | jq '{
  uptime: .uptime,
  connections: .connections,
  memory: .mem,
  opcounters: .opcounters
}'"
```

### Replicaset status (si está configurado)
```bash
docker exec -it mongodb mongosh --eval "
rs.status()"
```

### Logs del contenedor
```bash
docker logs mongodb -f --tail 100
```

### Índices (estadísticas)
```bash
docker exec -it mongodb mongosh --eval "
use database_name;
db.collection_name.getIndexes()"
```

---

## 7. Monitoreo Específico

### Lock status
```bash
docker exec -it mongodb mongosh --eval "
db.serverStatus().locks"
```

### Replication lag (si es secundario)
```bash
docker exec -it mongodb mongosh --eval "
rs.printSecondaryReplicationInfo()"
```

### Verificar tamaño de oplog
```bash
docker exec -it mongodb mongosh --eval "
use local;
db.oplog.rs.stats()"
```

---

## 8. Script de Monitoreo Completo

Crear archivo `monitor_mongodb.sh`:

```bash
#!/bin/bash
# monitor_mongodb.sh - Monitoreo de MongoDB

echo "======================================"
echo "🍃 MONITOREO MONGODB"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats mongodb --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v mongodb:/data busybox du -sh /data
echo ""
echo "Bases de datos:"
docker exec mongodb mongosh --eval "
db.adminCommand({listDatabases: 1, nameOnly: true}).databases.forEach(db => {
    const size = db.sizeOnDisk || 0;
    print(db.name + ': ' + (size/1024/1024/1024).toFixed(2) + ' GB');
})" 2>/dev/null
echo ""

echo "🧠 STATUS DE MEMORIA"
docker exec mongodb mongosh --eval "
let mem = db.serverStatus().mem;
print('Virtual: ' + mem.bits + ' bits');
print('Resident (RSS): ' + mem.resident + ' MB');
print('Mapped: ' + mem.mapped + ' MB');" 2>/dev/null
echo ""

echo "🔗 CONEXIONES"
docker exec mongodb mongosh --eval "
let conn = db.serverStatus().connections;
print('Current: ' + conn.current);
print('Available: ' + conn.available);" 2>/dev/null
echo ""

echo "⚡ OPERACIONES/seg"
docker exec mongodb mongosh --eval "
db.serverStatus().opcounters" 2>/dev/null
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_mongodb.sh
./monitor_mongodb.sh
```

---

## 9. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 75%** del límite
- **Conexiones > 80%** del máximo
- **Queries lentas > 100ms** frecuentemente

```bash
# Verificar queries lentas
docker exec mongodb mongosh --eval "
db.system.profile.find({millis: {'\$gt': 100}}).count()"
```

---

## 10. Optimizaciones

### Cambiar límite de memoria
```bash
docker update --memory 4g mongodb
docker restart mongodb
```

### Habilitar profiling para queries lentas
```bash
docker exec -it mongodb mongosh --eval "
db.setProfilingLevel(1, { slowms: 100 })"
```

### Crear índices (mejora performance)
```bash
docker exec -it mongodb mongosh --eval "
use database_name;
db.collection_name.createIndex({ field: 1 })"
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats mongodb` | Stats en tiempo real |
| `docker logs mongodb -f` | Logs |
| `docker exec mongodb mongosh --eval "db.version()"` | Versión |
| `docker volume inspect mongodb` | Info del volumen |
| `docker exec mongodb mongodump -o /tmp/dump` | Backup |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
