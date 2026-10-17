# 📈 VPS Scaling Strategies - n8n Workers

## Introducción

Este documento explica cómo escalar horizontalmente los workers en tu VPS de n8n de forma inteligente y sin downtime.

---

## 🎯 Conceptos Clave

### Queue Mode Escalabilidad

En Queue Mode de n8n:

```
┌─────────────────┐
│   n8n-web       │  ← Interfaz (Local)
│   :5678         │
└────────┬────────┘
         │ envía trabajos a
         │
    ┌────▼────────────────┐
    │  Redis Queue        │  ← Cola de trabajos
    │  (Servidor Externo) │
    └────┬───────────────┬┘
         │               │
    ┌────▼────────┐ ┌───▼────────┐
    │  Workers    │ │  Workers   │
    │  Local   │ │  VPS       │
    │  (2x)       │ │  (N x)     │
    └─────────────┘ └────────────┘
         │               │
         └───────┬───────┘
                 │ Resultado a
                 ▼
         ┌──────────────────┐
         │  PostgreSQL      │  ← BD de resultados
         │  (Servidor Ext.) │
         └──────────────────┘
```

### ¿Cuántos Workers Necesito?

```
Fórmula básica:
Workers = (Workflows Concurrentes Esperados) / (Concurrency por Worker)

Ejemplo:
- Esperos 20 workflows concurrentes
- Cada worker maneja 2 concurrently (WORKER_CONCURRENCY=2)
- Necesito: 20 / 2 = 10 workers
```

---

## 📊 Estrategia 1: Escalado Manual

### Aumentar Workers (Scale Up)

```bash
# En VPS - Aumentar a 10 workers
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=10

# Verificar
docker compose -f vps/docker-compose.vps.yml ps
```

**O desde PC:**

```bash
# Configurar SSH
export VPS_IP=192.168.1.50
export VPS_USER=ubuntu

# Escalar via vps-helper.sh
./script/vps-helper.sh scale 10
```

### Disminuir Workers (Scale Down)

```bash
# Reducir a 5 workers
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=5

# Verificar Estado
docker compose -f vps/docker-compose.vps.yml ps
```

### Monitoreo Durante Escalado

```bash
# En otra terminal - monitoring en vivo
./vps/script/maintenance.sh follow n8n-worker

# Ver logs
docker compose -f vps/docker-compose.vps.yml logs -f

# Ver estadísticas
watch -n 2 'docker compose -f vps/docker-compose.vps.yml ps'
```

---

## 🤖 Estrategia 2: Escalado Automático Basado en Cron

```bash
# Editar crontab
crontab -e

# Ejemplo: Escalar según hora del día

# Madrugada (3 AM - 6 AM): 2 workers
0 3 * * * cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=2

# Mañana (6 AM):  5 workers
0 6 * * * cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=5

# Mediodía (12 PM): 10 workers
0 12 * * * cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=10

# Tarde (6 PM): 15 workers
0 18 * * * cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=15

# Noche (9 PM): 8 workers
0 21 * * * cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=8

# Fin de semana: reducir
0 0 * * 6,0 cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=3
```

---

## 📡 Estrategia 3: Escalado Basado en Métricas

### Script Automático de Escalado

Crear `vps/script/auto-scale.sh`:

```bash
#!/bin/bash

# Verificar cola de trabajos y escalar automáticamente
# Ejecutar cada 5 minutos via cron

COMPOSE_FILE="vps/docker-compose.vps.yml"
MAX_WORKERS=20
MIN_WORKERS=2

# Verificar longitud de cola
QUEUE_LENGTH=$(docker compose -f "$COMPOSE_FILE" exec -T n8n-worker redis-cli LLEN "queue:default" 2>/dev/null || echo 0)

# Verificar workers actuales
CURRENT=$(docker compose -f "$COMPOSE_FILE" ps | wc -l)
CURRENT=$((CURRENT - 1))  # Restar header

# Lógica de escalado
if [ "$QUEUE_LENGTH" -gt 50 ] && [ "$CURRENT" -lt "$MAX_WORKERS" ]; then
    # Hay trabajo esperando y podemos escalar
    NEW_WORKERS=$((CURRENT + 5))
    [ "$NEW_WORKERS" -gt "$MAX_WORKERS" ] && NEW_WORKERS=$MAX_WORKERS
    
    echo "Escalando a $NEW_WORKERS workers (cola: $QUEUE_LENGTH)"
    docker compose -f "$COMPOSE_FILE" up -d --scale n8n-worker=$NEW_WORKERS
    
elif [ "$QUEUE_LENGTH" -lt 10 ] && [ "$CURRENT" -gt "$MIN_WORKERS" ]; then
    # Poco trabajo, escalar abajo
    NEW_WORKERS=$((CURRENT - 2))
    [ "$NEW_WORKERS" -lt "$MIN_WORKERS" ] && NEW_WORKERS=$MIN_WORKERS
    
    echo "Reduciendo a $NEW_WORKERS workers (cola: $QUEUE_LENGTH)"
    docker compose -f "$COMPOSE_FILE" up -d --scale n8n-worker=$NEW_WORKERS
fi
```

**Configurar en cron:**

```bash
# Cada 5 minutos
*/5 * * * * cd /opt/n8n && ./vps/script/auto-scale.sh >> /var/log/n8n/auto-scale.log 2>&1
```

---

## 💻 Estrategia 4: Escalado Remoto desde PC

### Usando vps-helper.sh

```bash
# Local - Configurar SSH
export VPS_IP=192.168.1.50
export VPS_USER=ubuntu

# Ver estado actual
./script/vps-helper.sh status

# Escalar a distintas cantidades según necesidad
./script/vps-helper.sh scale 5     # 5 workers
./script/vps-helper.sh scale 10    # 10 workers
./script/vps-helper.sh scale 20    # 20 workers

# Ver cómo está ahora
./script/vps-helper.sh status

# Ver logs de escalado
./script/vps-helper.sh logs-workers
```

---

## 📊 Monitoreo Durante Escalado

### Métricas Clave

```bash
# 1. Longitud de la cola
./vps/script/maintenance.sh stats

# 2. CPU y Memoria
docker stats --no-stream

# 3. Logs en vivo
./vps/script/maintenance.sh follow n8n-worker

# 4. Contenedores activos
docker compose -f vps/docker-compose.vps.yml ps
```

### Dashboard de Escalado

```bash
# Terminal 1: Queue length
watch -n 5 'docker compose -f vps/docker-compose.vps.yml ps | wc -l'

# Terminal 2: Logs
./vps/script/maintenance.sh follow n8n-worker

# Terminal 3: Recursos
watch -n 2 'docker stats --no-stream'
```

---

## 🎯 Tabla de Referencia: Workers recomendados

| Escenario | Workers Recomendados | WORKER_CONCURRENCY | Justificación |
|-----------|---------------------|-------------------|-------------|
| Desarrollo/Testing | 1-2 | 1 | Bajo volumen, fácil debugging |
| Producción Baja | 3-5 | 2 | <10 workflows/dia |
| Producción Media | 5-10 | 2 | 10-100 workflows/dia |
| Producción Alta | 10-20 | 3-4 | 100-1000 workflows/dia |
| Producción Crítica | 20+ | 4+ | 1000+ workflows/dia |

---

## ⚖️ Consideraciones de Recursos

### Memoria por Worker

```
Cada worker consume típicamente:
- Base: 200-300 MB
- Con workflow ejecutándose: +100-200 MB
- Total por worker: 300-500 MB

Ejemplo:
- 10 workers × 400 MB = 4 GB
- 20 workers × 400 MB = 8 GB
```

### CPU por Worker

```
Cada worker puede usar:
- En idle: 1-5% de un core
- Ejecutando: 25-100% de un core

Recomendación:
- 1 core del VPS = 4-5 workers máximo
- 4 cores = 16-20 workers máximo
```

### Configuración de Recursos

```yaml
# vps/docker-compose.vps.yml - Agregar limits

services:
  n8n-worker:
    deploy:
      resources:
        limits:
          cpus: '1'          # 1 core máximo por worker
          memory: 512M       # 512 MB máximo por worker
        reservations:
          cpus: '0.5'        # Reservar 0.5 cores
          memory: 256M       # Reservar 256 MB
```

---

## 🚨 Límites de Escalabilidad

### Máximo de Workers por VPS

```
Limitaciones:
- Redis: Soporta 10,000+ conexiones (no es problema)
- PostgreSQL: Típicamente soporta 100+ conexiones (con n8n configured)
- Disco: Logs pueden crecer 50MB-500MB por día con muchos workers
- Red: Bandwidth puede ser cuello de botella en operaciones de I/O pesadas
```

### Escalado Más Allá

```
Si necesitas > 30 workers:

Opción 1: Múltiples VPS
  - VPS 1: 20 workers
  - VPS 2: 20 workers
  - Todos conectan a MISMO Redis y PostgreSQL

Opción 2: Kubernetes
  - Usar Helm charts de n8n
  - Auto-scaling basado en métricas
  - Load balancing automático

Opción 3: Cloud Auto-Scaling
  - AWS ECS
  - Google Cloud Run
  - Azure Container Instances
```

---

## 🔄 Graceful Scaling

### Scale Down Sin Perder Trabajos

```bash
# CORRECTO: Docker gracefully respeta trabajos en proceso
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=5

# EVITAR: Kill de contenedores (pierde trabajo en proceso)
docker kill <container-id>
```

### Health Check Durante Escalado

```bash
# Verificar que todo está bien después de escalar
./vps/script/health-check.sh

# Esperar a que workers se conecten a Redis
sleep 10
docker compose -f vps/docker-compose.vps.yml logs n8n-worker | tail -20
```

---

## 📋 Plan de Escalado Recomendado

### Semana 1: Setup Inicial
```bash
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=3
```

### Semana 2-3: Monitoreo
```bash
# Observar:
- Carga promedio de workers
- Tiempo de ejecución de workflow
- Memoria consumida
./vps/script/maintenance.sh resources
```

### Semana 4+: Optimización
```bash
# Escalar según patrones observados
# Ejemplo: Si hay picos a mediodía
- 3 workers madrugada
- 10 workers mediodía
- 5 workers noche
```

---

## 🔗 Referencias

- [MAINTENANCE_GUIDE.md](MAINTENANCE_GUIDE.md) - Mantenimiento
- [HEALTH_MONITORING.md](HEALTH_MONITORING.md) - Monitoreo
- [TROUBLESHOOTING_VPS.md](TROUBLESHOOTING_VPS.md) - Problemas
- [../../doc/SCRIPT_LOCATION.md](../../doc/SCRIPT_LOCATION.md) - Scripts en general

---

**¡Escalando inteligentemente! 📈**
