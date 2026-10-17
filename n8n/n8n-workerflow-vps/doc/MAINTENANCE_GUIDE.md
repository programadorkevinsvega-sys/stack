# 🔧 VPS Maintenance Guide - maintenance.sh

## Introducción

El script `maintenance.sh` es tu herramienta completa para tareas de mantenimiento en el VPS. Incluye 10+ comandos para limpiar, monitorear, analizar y actualizar n8n.

---

## 📋 Comandos Disponibles

### 🧹 LIMPIEZA

#### 1. Cleanup - Limpiar Docker

```bash
./vps/script/maintenance.sh cleanup
```

**Qué hace:**
- Elimina imágenes dangling (no usadas)
- Elimina volúmenes dangling
- Elimina redes dangling
- Elimina builds dangling

**Salida:**
```
✅ Eliminadas 3 imágenes no utilizadas
✅ Eliminados 2 volúmenes no utilizados
✅ Eliminadas 1 redes no utilizadas
✅ Eliminados 0 builds no utilizados
✅ Limpieza completada
```

**Cuándo ejecutar:** Semanalmente o cuando disco está lleno

---

#### 2. Clean-Logs - Limpiar Logs Antiguos

```bash
# Limpiar logs más antiguos que 30 días (default)
./vps/script/maintenance.sh clean-logs

# Limpiar logs más antiguos que 7 días
./vps/script/maintenance.sh clean-logs 7

# Limpiar logs más antiguos que 1 día
./vps/script/maintenance.sh clean-logs 1
```

**Qué hace:**
- Limpia system logs antiguos (journalctl)
- Trunca logs de contenedores Docker
- Espacio recuperado: típicamente 500MB - 2GB

**Cuándo ejecutar:** Mensualmente o cuando espacio en disco sea crítico

---

### 📊 MONITOREO

#### 3. Stats - Estadísticas en Tiempo Real

```bash
./vps/script/maintenance.sh stats
```

**Muestra:**
```
CPU         MEMORY          NET I/O     BLOCK I/O   PIDS
2.15%       356.2MiB / 16GiB   0B / 0B    0B / 0B    45
```

**Información:**
- CPU % actual
- Memoria (usada / total)
- Network I/O
- Block I/O
- PIDs (procesos)

**Cuándo usar:** Verificación rápida de carga

---

#### 4. Logs - Ver Logs de Servicios

```bash
# Ver últimas 100 líneas (default)
./vps/script/maintenance.sh logs

# Ver últimas 50 líneas de n8n-worker
./vps/script/maintenance.sh logs n8n-worker 50

# Ver últimas 200 líneas
./vps/script/maintenance.sh logs n8n-worker 200
```

**Opciones de servicio:**
- `n8n-worker` - Workers principales
- `n8n-web` - Si existe

**Uso:** Debugging, verificar errores

---

#### 5. Follow - Logs en Tiempo Real

```bash
# Seguir logs de n8n-worker
./vps/script/maintenance.sh follow n8n-worker

# Presiona Ctrl+C para detener
```

**Muestra:**
```
n8n-worker-1 | [INFO] Workflow started: #abc123
n8n-worker-2 | [INFO] Execution completed successfully
n8n-worker-1 | [ERROR] Connection timeout to Redis
```

**Uso:** Debugging en vivo, troubleshooting

---

### 💾 BACKUPS Y RETENCIÓN

#### 6. Rotate-Backups - Rotación de Backups

```bash
# Rotar backups (mantener últimos 30 días)
./vps/script/maintenance.sh rotate-backups

# Rotar y mantener últimos 90 días
RETENTION_DAYS=90 ./vps/script/maintenance.sh rotate-backups

# Con variable de entorno
export RETENTION_DAYS=60
./vps/script/maintenance.sh rotate-backups
```

**Qué hace:**
- Elimina backups más antiguos que RETENTION_DAYS
- Muestra número de backups eliminados
- Muestra espacio ahorrado y estado actual

**Salida:**
```
✅ Eliminados 3 backups antiguos
✅ Backups guardados: 12
✅ Tamaño total: 5.2G
```

**Cuándo ejecutar:** Mensualmente

---

### 🖥️ INFORMACIÓN DEL SISTEMA

#### 7. System-Info - Información del Sistema

```bash
./vps/script/maintenance.sh system-info
```

**Muestra:**
- OS (Ubuntu, Debian, etc.)
- CPUs disponibles
- RAM total
- Disco total y usado
- Uptime
- Versiones de Docker

**Ejemplo:**
```
Distribución: Ubuntu 20.04.5 LTS
Cores: 4
Total: 16G, Usada: 8G, Disponible: 8G
Disco: Tamaño: 100G, Usado: 45G, Disponible: 55G, Uso: 45%
Uptime: 45 days, 3 hours, 22 minutes
Docker: 20.10.21
Docker Compose: v2.15.1
```

**Uso:** Documentación, auditoría, troubleshooting

---

#### 8. Resources - Análisis de Recursos

```bash
./vps/script/maintenance.sh resources
```

**Top 10 contenedores por memoria:**
```
CONTAINER           MEMORY
n8n-worker-1        356.2MiB
n8n-worker-2        298.5MiB
n8n-worker-3        267.1MiB
```

**Volúmenes más grandes:**
```
n8n-data-volume     4.2G
postgres-backups    1.8G
redis-backup        256MB
```

**Uso:** Identificar cuellos de botella

---

#### 9. DB-Stats - Estadísticas de Base de Datos

```bash
./vps/script/maintenance.sh db-stats
```

**Nota:** Se asume PostgreSQL accesible. Requiere:
- `psql` cliente instalado
- Credenciales en .env
- Conectividad a DB

**Vista manual:**
```bash
psql -h <DB_HOST> -U n8n_user -d n8n -c \
  "SELECT * FROM pg_stat_database WHERE datname='n8n';"
```

---

### ♻️ MANTENIMIENTO

#### 10. Update - Descargar Nuevas Imágenes

```bash
./vps/script/maintenance.sh update
```

**Qué hace:**
- Descarga últimas imágenes de n8n desde Docker Hub
- No reinicia automáticamente

**Para aplicar actualización:**
```bash
docker compose -f vps/docker-compose.vps.yml up -d
```

---

#### 11. Validate - Validar Configuración

```bash
./vps/script/maintenance.sh validate
```

**Verifica:**
- Sintaxis de docker-compose.yml
- Servicios definidoscorrectamente
- Volúmenes nombrados
- Redes

**Salida exitosa:**
```
✅ Sintaxis válida
✅ NAME              IMAGE           STATUS
   n8n-worker-1     n8nio/n8n       Up 2 days
   n8n-worker-2     n8nio/n8n       Up 2 days
✅ Validación completada
```

---

## 🔄 Guía de Automatización

### Cron Jobs Recomendados

```bash
# Editar crontab
crontab -e

# Agregar ejecuciones periódicas:

# DIARIA (6 AM)
# Stats del sistema
0 6 * * * cd /opt/n8n && ./vps/script/maintenance.sh system-info >> /var/log/n8n/maintenance.log 2>&1

# SEMANAL (Domingo 2 AM)
# Limpieza de Docker
0 2 * * 0 cd /opt/n8n && ./vps/script/maintenance.sh cleanup >> /var/log/n8n/maintenance.log 2>&1

# MENSUAL (1st, 2 AM)
# Rotación de backups
0 2 1 * * cd /opt/n8n && ./vps/script/maintenance.sh rotate-backups >> /var/log/n8n/maintenance.log 2>&1

# CADA 4 HORAS
# Limpiar logs
0 */4 * * * cd /opt/n8n && ./vps/script/maintenance.sh clean-logs 7 >> /var/log/n8n/maintenance.log 2>&1

# SEMANA: Actualizar imágenes (Domingo 1 AM)
# Descargar pero no aplicar aún
0 1 * * 0 cd /opt/n8n && ./vps/script/maintenance.sh update >> /var/log/n8n/maintenance.log 2>&1
```

### Ver Histórico de Mantenimiento

```bash
# Todos los logs
tail -f /var/log/n8n/maintenance.log

# Por comando
grep "cleanup" /var/log/n8n/maintenance.log
grep "clean-logs" /var/log/n8n/maintenance.log

# Últimos cambios
tail -50 /var/log/n8n/maintenance.log
```

---

## 🛠️ Variables de Entorno Configurables

```bash
# Cambiar directorio de logs
LOG_DIR=/custom/logs ./vps/script/maintenance.sh cleanup

# Cambiar directorio de backups
BACKUP_DIR=/data/backups ./vps/script/maintenance.sh rotate-backups

# Cambiar días de retención
RETENTION_DAYS=90 ./vps/script/maintenance.sh rotate-backups

# Cambiar archivo compose
COMPOSE_FILE=/opt/n8n/vps/docker-compose.vps.yml ./vps/script/maintenance.sh validate

# Todos juntos
LOG_DIR=/logs BACKUP_DIR=/backups RETENTION_DAYS=45 COMPOSE_FILE=/opt/n8n/vps/docker-compose.vps.yml \
  ./vps/script/maintenance.sh cleanup
```

---

## 📈 Plan de Mantenimiento Semanal

### Lunes 10 AM
```bash
# Verificar estado
./vps/script/maintenance.sh health-check   # (desde vps/script/)

# Ver estadísticas
./vps/script/maintenance.sh stats
```

### Miércoles 2 AM (Automático vía Cron)
```bash
./vps/script/maintenance.sh cleanup
./vps/script/maintenance.sh clean-logs 7
```

### Viernes 5 PM
```bash
# Análisis de recursos
./vps/script/maintenance.sh resources

# Ver si hay logs de error
grep -i "error" /var/log/n8n/maintenance.log | tail -20
```

### Domingo 1 AM (Automático vía Cron)
```bash
./vps/script/maintenance.sh update          # Descargar nuevas versiones
./vps/script/maintenance.sh rotate-backups  # Rotar backups viejos
./vps/script/maintenance.sh system-info     # Snapshot del sistema
```

---

## 🆘 Situaciones Comunes

### Disco Lleno

```bash
# 1. Ver qué está usando espacio
./vps/script/maintenance.sh resources

# 2. Limpiar logs agresivamente
./vps/script/maintenance.sh clean-logs 1

# 3. Limpiar Docker
./vps/script/maintenance.sh cleanup

# 4. Rotar backups
./vps/script/maintenance.sh rotate-backups

# Si sigue lleno:
du -sh /opt/n8n/* | sort -hr
```

### Memoria Alta

```bash
# 1. Ver consumo
./vps/script/maintenance.sh resources
./vps/script/maintenance.sh stats

# 2. Reducir workers
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=2

# 3. Reiniciar si es necesario
docker compose -f vps/docker-compose.vps.yml restart
```

### Actualizar n8n

```bash
# 1. Descargar nuevas imágenes
./vps/script/maintenance.sh update

# 2. Esperar a que no haya trabajos en desarrollo
./vps/script/maintenance.sh stats

# 3. Aplicar actualización
docker compose -f vps/docker-compose.vps.yml up -d

# 4. Verificar
docker compose -f vps/docker-compose.vps.yml logs n8n-worker | head -20
```

---

## 🔗 Referencias

- [HEALTH_MONITORING.md](HEALTH_MONITORING.md) - Health check en profundidad
- [SCALING_STRATEGIES.md](SCALING_STRATEGIES.md) - Escalabilidad
- [TROUBLESHOOTING_VPS.md](TROUBLESHOOTING_VPS.md) - Problemas avanzados
- [QUICK_START_VPS.md](QUICK_START_VPS.md) - Setup inicial

---

**¡Manteniendo tu infraestructura limpia y optimizada! 🚀**
