# 🏥 VPS Health Monitoring - Guía Completa

## Introducción

Este documento explica cómo usar `health-check.sh` y `maintenance.sh` para monitorear la salud integral del VPS con n8n workers.

---

## 🔍 Health Check Overview

El script `health-check.sh` realiza **16+ verificaciones** de salud del sistema:

```bash
cd /opt/n8n
./vps/script/health-check.sh
```

---

## ✅ Verificaciones del Sistema

### 1. Docker Instalado
```bash
# Verifica:
- Docker está instalado
- Docker daemon está corriendo
- Docker Compose disponible

# Si falla:
sudo apt-get install docker.io docker-compose
sudo systemctl start docker
sudo usermod -aG docker $USER
```

### 2. Contenedores n8n
```bash
# Verifica:
- Contenedores n8n están levantados
- Número de instancias correctas

# Ver detalles:
docker compose -f vps/docker-compose.vps.yml ps
```

### 3. Espacio en Disco
```bash
# Alerta si:
- > 90% utilizado = ERROR
- > 75% utilizado = WARNING

# Limpiar si es necesario:
./vps/script/maintenance.sh cleanup
./vps/script/maintenance.sh clean-logs 7
```

### 4. Memoria Disponible
```bash
# Alerta si:
- < 10% disponible = ERROR
- < 25% disponible = WARNING

# Ver uso con:
free -h
docker stats --no-stream
```

### 5. Carga de CPU
```bash
# Alerta si:
- Carga promedio > (cores × 2)

# Ver cores:
nproc

# Ver carga actual:
uptime -p
top -bn1 | head -3
```

### 6. Conectividad de Red
```bash
# Verifica:
- Conexión a internet disponible
- DNS resolviendo correctamente

# Testear:
curl -I https://www.google.com
ping 8.8.8.8
```

### 7. Logs Sin Errores
```bash
# Verifica:
- Últimas 100 líneas de logs
- Cuenta de errores críticos

# Ver logs completos:
docker compose -f vps/docker-compose.vps.yml logs --tail 100
```

---

## 📊 Opciones de Health Check

### Ejecución Básica
```bash
./vps/script/health-check.sh
```

### Con Logs en Tiempo Real
```bash
./vps/script/health-check.sh --logs
```

### Con Archivo Compose Específico
```bash
./vps/script/health-check.sh -c /ruta/a/docker-compose.vps.yml
```

### Combinadas
```bash
./vps/script/health-check.sh --compose vps/docker-compose.vps.yml --logs
```

---

## 📈 Interpretación de Resultados

### ✅ Checks Pasados (Green)
```
✅ Docker instalado
✅ Docker daemon está corriendo
✅ Docker Compose v2.x.x
✅ Encontrados 3 contenedores n8n
✅ Espacio en disco OK: 45% utilizado
✅ Memoria disponible OK: 60%
✅ Carga de CPU OK: 1.5
✅ Conectividad de red OK
✅ Logs limpios
```

**Significado:** Sistema en estado óptimo. ✅ SISTEMA SALUDABLE

---

### ⚠️ Warnings (Yellow)
```
⚠️  Memoria disponible baja: 20%
⚠️  Carga de CPU alta: 7.2 (threshold: 8)
⚠️  No hay conectividad a internet
```

**Acciones recomendadas:**
- Monitor más frecuentemente
- Revisar procesos que consumen recursos
- Reducir workers temporalmente
- Investigar causa raíz

---

### ❌ Errores (Red)
```
❌ Disco lleno: 95% utilizado
❌ Memoria disponible muy baja: 5%
❌ Docker daemon no está corriendo
❌ No hay contenedores n8n ejecutándose
```

**Acciones INMEDIATAS:**
- Intervenir manualmente
- Limpiar espacio disk (si aplica)
- Reiniciar Docker (si aplica)
- Escalar down si es necesario

---

## 🔄 Monitoreo Automático

### Configurar Cron para Health Checks Periódicos

```bash
# Editar crontab
crontab -e

# Agregar ejecuciones periódicas:

# Cada 30 minutos
*/30 * * * * cd /opt/n8n && ./vps/script/health-check.sh >> /var/log/n8n/health-check.log 2>&1

# Cada 4 horas
0 */4 * * * cd /opt/n8n && ./vps/script/health-check.sh --logs >> /var/log/n8n/health-check.log 2>&1

# Diariamente a las 6 AM
0 6 * * * cd /opt/n8n && ./vps/script/health-check.sh >> /var/log/n8n/health-check.log 2>&1
```

### Ver Histórico de Cambios

```bash
# Logs se guardan en:
/var/log/n8n/health-check-YYYYMMDD-HHMMSS.log

# Ver último check
tail -f /var/log/n8n/health-check-*.log

# Buscar problemas
grep -i "error" /var/log/n8n/health-check-*.log
grep "❌" /var/log/n8n/health-check-*.log
```

---

## 🎯 Alertas Específicas y Soluciones

### Disco Lleno (95%+)

**Causas comunes:**
- Logs de Docker grandes
- Backups no rotados
- Volúmenes de datos no limpiados

**Solución rápida:**
```bash
# Limpiar logs
./vps/script/maintenance.sh clean-logs 7

# Limpiar Docker
./vps/script/maintenance.sh cleanup

# Rotar backups viejos
./vps/script/maintenance.sh rotate-backups
```

**Si sigue lleno:**
```bash
# Ver qué está usando espacio
du -sh /* | sort -hr
find /var/lib/docker -type f -size +100M

# Limpiar manualmente (último recurso)
docker system prune -a --volumes
```

---

### Memoria Baja (< 25%)

**Causas comunes:**
- Demasiados workers corriendo
- Memoria leak en algún contenedor
- Otros procesos del sistema

**Solución:**
```bash
# Ver consumo por contenedor
docker stats --no-stream

# Reducir workers temporalmente
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=2

# Reiniciar contenedores
docker compose -f vps/docker-compose.vps.yml restart

# Aumentar swap (último recurso)
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```

---

### CPU Carga Alta

**Causas comunes:**
- Workflows complejos ejecutándose
- Demasiadas replicas de workers
- Query ineficiente a BD

**Solución:**
```bash
# Ver procesos que consumen CPU
top

# Análisis de recursos
./vps/script/maintenance.sh resources

# Reducir workers
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=2

# Esperar a que bajen los trabajos
# Ver cola:
./vps/script/maintenance.sh stats
```

---

### Sin Conectividad a Redis/PostgreSQL

**Causas comunes:**
- Firewall bloqueando puertos
- Servicios externos caídos
- Red virtual desconectada

**Verificación:**
```bash
# Test directo
redis-cli -h <REDIS_IP> ping
psql -h <DB_IP> -U n8n_user -d n8n -c "SELECT 1;"

# Con Docker
docker run --rm redis:7-alpine redis-cli -h <REDIS_IP> ping
docker run --rm postgres:15-alpine psql -h <DB_IP> -c "SELECT 1;"

# Verificar .env
cat vps/.env | grep -E "REDIS_HOST|POSTGRESDB_HOST"
```

---

### Workers No Procesan (Logs Limpios pero Sin Trabajo)

**Causas comunes:**
- No hay trabajos en la cola
- Workers no conectados a Redis
- N8N_ENCRYPTION_KEY incorrecta

**Diagnóstico:**
```bash
# Ver logs en vivo
./vps/script/maintenance.sh follow n8n-worker

# Verificar cola
./vps/script/maintenance.sh stats

# Ver si está escuchando Redis
docker compose -f vps/docker-compose.vps.yml logs n8n-worker | grep -i "queue\|redis"

# Reiniciar si es necesario
docker compose -f vps/docker-compose.vps.yml restart
```

---

## 📊 Dashboard de Monitoreo

### Crear Monitoreo Visual

```bash
# Instalar tmux si no lo tienes
sudo apt-get install tmux

# Crear session con múltiples windows
tmux new-session -d -s n8n-monitoring -x 200 -y 50

# Window 0: Health check en loop
tmux send-keys -t n8n-monitoring "while true; do clear; cd /opt/n8n && ./vps/script/health-check.sh; sleep 10; done" Enter

# Window 1: Logs en vivo
tmux new-window -t n8n-monitoring
tmux send-keys -t n8n-monitoring "cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml logs -f --tail 50" Enter

# Window 2: Stats en vivo
tmux new-window -t n8n-monitoring
tmux send-keys -t n8n-monitoring "watch -n 5 'docker stats --no-stream'" Enter

# Conectar al monitoring
tmux attach -t n8n-monitoring

# Navegar con: Ctrl+B + N (next window)
```

---

## 📈 Métricas Clave a Monitorear

| Métrica | Verde | Amarillo | Rojo |
|---------|-------|----------|------|
| Disco Usado | < 75% | 75-90% | > 90% |
| Memoria Libre | > 25% | 10-25% | < 10% |
| CPU Load | < 1×cores | 1-2×cores | > 2×cores |
| Contenedores Up | 3+ | 1-2 | 0 |
| Logs (últimas 100) | 0-1 errores | 2-10 errores | > 10 errores |
| Redis Conectado | ✅ | - | ❌ |
| DB Conectada | ✅ | - | ❌ |

---

## 🔗 Referencias

- [MAINTENANCE_GUIDE.md](MAINTENANCE_GUIDE.md) - Tareas de mantenimiento
- [SCALING_STRATEGIES.md](SCALING_STRATEGIES.md) - Escalabilidad
- [TROUBLESHOOTING_VPS.md](TROUBLESHOOTING_VPS.md) - Problemas avanzados
- [../SCRIPTS_SEPARATION.md](../doc/SCRIPTS_SEPARATION.md) - Scripts en general

---

## 💡 Tips Pro

```bash
# Alert en caso de problemas
# Agregar a cron con opción de enviar email:
0 */4 * * * cd /opt/n8n && ./vps/script/health-check.sh 2>&1 | grep -E "ERROR|FAILED" && mail -s "n8n VPS Alert" admin@example.com

# Keepalive del script
# Ejecutar en background con nohup:
nohup bash -c 'while true; do ./vps/script/health-check.sh >> /var/log/n8n/health.log 2>&1; sleep 600; done' &

# Integration con uptime monitoring services
# Usar curl para reportar status a servicios como:
# - Grafana
# - Prometheus
# - Custom API
```

---

**¡Manteniendo la salud de tu VPS! 🏥✅**
