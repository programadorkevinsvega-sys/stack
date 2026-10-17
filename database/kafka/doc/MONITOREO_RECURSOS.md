# 📊 Monitoreo de Recursos - Kafka

Documentación específica para monitorear el consumo de recursos del contenedor Kafka.

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats kafka --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats kafka --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect kafka --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats kafka --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Memoria del broker (dentro del contenedor)
```bash
docker exec kafka jps -m | grep Kafka
```

### Detalles de memoria JVM
```bash
docker exec kafka jstat -gc -h5 $(docker exec kafka jps | grep Kafka | awk '{print $1}') 1000 10
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep kafka
```

### Tamaño del volumen
```bash
docker run --rm -v kafka:/data busybox du -sh /data
```

### Particiones almacenadas
```bash
docker exec kafka du -sh /var/lib/kafka/data/*
```

### Info de logs (storage)
```bash
docker exec kafka ls -lh /var/lib/kafka/data/ | head -20
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats kafka --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Procesos de Kafka
```bash
docker exec kafka ps aux | grep kafka
```

### Con top dentro del contenedor
```bash
docker exec kafka top -b -n 1 | head -20
```

---

## 5. Tópicos y Particiones

### Listar tópicos
```bash
docker exec kafka kafka-topics.sh --list --bootstrap-server localhost:9092
```

### Información detallada de un tópico
```bash
docker exec kafka kafka-topics.sh --describe --topic topic_name --bootstrap-server localhost:9092
```

### Todos los tópicos con detalles
```bash
docker exec kafka kafka-topics.sh --describe --bootstrap-server localhost:9092
```

### Tamaño de tópicos
```bash
docker exec kafka bash -c '
for topic in $(kafka-topics.sh --list --bootstrap-server localhost:9092); do
  size=$(du -sh /var/lib/kafka/data/$topic-* 2>/dev/null | awk "{print \$1}")
  echo "$topic: $size"
done
'
```

---

## 6. Consumidores

### Listar grupos de consumidores
```bash
docker exec kafka kafka-consumer-groups.sh --list --bootstrap-server localhost:9092
```

### Info de grupo de consumidor
```bash
docker exec kafka kafka-consumer-groups.sh --describe \
  --group group_name --bootstrap-server localhost:9092
```

### Lag de consumidor (retraso)
```bash
docker exec kafka kafka-consumer-groups.sh --describe \
  --group group_name --bootstrap-server localhost:9092 | grep LAG
```

### Todos los consumidores con lag
```bash
docker exec kafka bash -c '
for group in $(kafka-consumer-groups.sh --list --bootstrap-server localhost:9092); do
  echo "=== Grupo: $group ==="
  kafka-consumer-groups.sh --describe --group $group --bootstrap-server localhost:9092 | tail -n +2
done
'
```

---

## 7. Brokers y Cluster

### Info de brokers
```bash
docker exec kafka kafka-broker-api-versions.sh --bootstrap-server localhost:9092
```

### Metadata del cluster
```bash
docker exec kafka kafka-metadata.sh --snapshot /var/lib/kafka/data/__cluster_metadata-0/00000000000000000000.log
```

### Puertos escuchando
```bash
docker exec kafka netstat -tuln | grep LISTEN
```

---

## 8. Performance y Throughput

### Productor de prueba
```bash
docker exec kafka kafka-producer-perf-test.sh \
  --topic test-perf \
  --num-records 100000 \
  --record-size 1024 \
  --throughput -1 \
  --producer-props bootstrap.servers=localhost:9092
```

### Consumidor de prueba
```bash
docker exec kafka kafka-consumer-perf-test.sh \
  --broker-list localhost:9092 \
  --topic test-perf \
  --messages 100000 \
  --threads 1
```

### Ver configuración de broker
```bash
docker exec kafka kafka-configs.sh --describe \
  --bootstrap-server localhost:9092 \
  --entity-type brokers \
  --entity-name 0
```

---

## 9. Logs y Debugging

### Logs del contenedor
```bash
docker logs kafka -f --tail 100
```

### Logs de broker (dentro del contenedor)
```bash
docker exec kafka tail -f /opt/kafka/logs/server.log
```

### Verificar integridad de logs
```bash
docker exec kafka kafka-log-validation.sh \
  --deep-iteration \
  --index-sanity-checks \
  --files /var/lib/kafka/data/topic-0/*
```

---

## 10. Script de Monitoreo Completo

Crear archivo `monitor_kafka.sh`:

```bash
#!/bin/bash
# monitor_kafka.sh - Monitoreo de Kafka

BS="localhost:9092"

echo "======================================"
echo "📨 MONITOREO KAFKA"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats kafka --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v kafka:/data busybox du -sh /data
echo ""
echo "Por partición:"
docker exec kafka du -sh /var/lib/kafka/data/* | sort -hr | head -10
echo ""

echo "🧠 MEMORIA (JVM)"
docker exec kafka jps -m | grep Kafka
echo ""

echo "📦 TÓPICOS"
TOPICS=$(docker exec kafka kafka-topics.sh --list --bootstrap-server $BS)
echo "Total de tópicos: $(echo "$TOPICS" | wc -l)"
echo "Tópicos:"
echo "$TOPICS" | head -5
echo ""

echo "👥 GRUPOS DE CONSUMIDORES"
GROUPS=$(docker exec kafka kafka-consumer-groups.sh --list --bootstrap-server $BS)
echo "Total de grupos: $(echo "$GROUPS" | wc -l)"
echo "Primeros 5 grupos:"
echo "$GROUPS" | head -5
echo ""

echo "⏱️  LAG DE CONSUMIDOR (primeros 3 grupos)"
for group in $(docker exec kafka kafka-consumer-groups.sh --list --bootstrap-server $BS | head -3); do
  echo "Grupo: $group"
  docker exec kafka kafka-consumer-groups.sh --describe \
    --group $group --bootstrap-server $BS | awk 'NR>1 {print $0}' | tail -1
done
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_kafka.sh
./monitor_kafka.sh
```

---

## 11. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 85%** del límite
- **Lag de consumidor > 10000** mensajes
- **Replicación rezagada** (ISR < replicas)
- **Disk space < 20%**

```bash
# Verificar ISR status
docker exec kafka kafka-topics.sh --describe --bootstrap-server localhost:9092 | grep -v "Leader:"
```

---

## 12. Optimizaciones

### Aumentar límite de memoria
```bash
docker update --memory 4g kafka
docker restart kafka
```

### Aumentar JVM heap
```bash
# En docker-compose.yml, agregar:
# environment:
#   KAFKA_HEAP_OPTS: "-Xmx2G -Xms2G"
```

### Crear tópico optimizado
```bash
docker exec kafka kafka-topics.sh --create \
  --topic optimized-topic \
  --partitions 6 \
  --replication-factor 1 \
  --bootstrap-server localhost:9092 \
  --config retention.ms=86400000 \
  --config compression.type=snappy
```

### Limpiar tópico de prueba
```bash
docker exec kafka kafka-topics.sh --delete \
  --topic test-perf \
  --bootstrap-server localhost:9092
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats kafka` | Stats en tiempo real |
| `docker logs kafka -f` | Logs |
| `docker exec kafka kafka-topics.sh --list` | Listar tópicos |
| `docker exec kafka kafka-consumer-groups.sh --list` | Listar consumidores |
| `docker exec kafka kafka-producer-perf-test.sh` | Test de carga |
| `docker exec kafka kafka-topics.sh --describe` | Detalles de tópicos |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
