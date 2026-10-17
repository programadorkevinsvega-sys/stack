# 🚀 VPS Quick Start - n8n Workers Deployment

## ⚡ Inicio Rápido en 10 Minutos

### Requisitos Previos

```bash
# En tu VPS, verificar:
✅ OS: Ubuntu 20.04+ o Debian 11+
✅ Docker 20.10+ instalado
✅ Docker Compose 2.0+ instalado
✅ 4GB+ RAM disponible
✅ 20GB+ disco disponible
✅ Conectividad a Redis externo (IP:6379)
✅ Conectividad a PostgreSQL externo (IP:5432)
```

---

## 🔧 Paso 1: Copiar Archivos a VPS

**Desde tu PC:**

```bash
# Opción 1: SCP
scp -r vps/ usuario@VPS_IP:/opt/n8n/
scp -r doc/ usuario@VPS_IP:/opt/n8n/       # Opcional: también docs

# Opción 2: Git clone
ssh usuario@VPS_IP "cd /opt && git clone <repo-url> n8n"
```

---

## 📁 Paso 2: Estructura en VPS

```bash
ssh usuario@VPS_IP

cd /opt/n8n
ls -la

# Deberías ver:
vps/
├── docker-compose.vps.yml
├── .env.example.vps
├── script/
│   ├── health-check.sh
│   └── maintenance.sh
└── doc/                            ← NUEVO

doc/
├── QUICK_START.md                  ← LOCAL
├── QUICK_START_VPS.md              ← TÚ ESTÁS AQUÍ
└── ...
```

---

## 🔐 Paso 3: Configurar Variables de Entorno

```bash
# En VPS
cd /opt/n8n/vps

# Copiar template
cp .env.example.vps .env

# Editar con tus valores
nano .env
```

**Variables CRÍTICAS a configurar:**

```env
# 🔐 ENCRIPTACIÓN (DEBE matchear tu máquina local)
N8N_ENCRYPTION_KEY=xxxxxxxxxxxxx  # Mismo que en PC
# Generar si aún no tienes: openssl rand -base64 32

# 📡 REDIS EXTERNO
QUEUE_BULL_REDIS_HOST=192.168.1.50    # IP donde corre Redis
QUEUE_BULL_REDIS_PORT=6379

# 🗄️ POSTGRESQL EXTERNO
DB_POSTGRESDB_HOST=192.168.1.50        # IP donde corre PostgreSQL
DB_POSTGRESDB_PORT=5432
DB_POSTGRESDB_USER=n8n_user
DB_POSTGRESDB_PASSWORD=tu_password_segura
DB_POSTGRESDB_DATABASE=n8n

# ⏰ ZONA HORARIA (DEBE matchear PC)
GENERIC_TIMEZONE=America/Bogota

# 🌐 ACCESO
N8N_HOST=45.32.100.5                   # IP pública del VPS
N8N_PORT=5678

# 🔄 WORKERS
WORKER_CONCURRENCY=2                   # Trabajos por worker
WORKER_REPLICAS=3                      # Número inicial de workers
```

**⚠️ IMPORTANTE:**
- `N8N_ENCRYPTION_KEY` debe ser **IDÉNTICA** a la del PC
- `QUEUE_BULL_REDIS_HOST` debe ser **MISMA IP** que en PC
- `DB_POSTGRESDB_HOST` debe ser **MISMA IP** que en PC

---

## ✅ Paso 4: Verificar Conectividad

Ejecutar **EN el VPS** para verificar que puede alcanzar servicios externos:

```bash
cd /opt/n8n/vps

# Verificar Redis
redis-cli -h 192.168.1.50 ping
# Respuesta esperada: PONG

# Verificar PostgreSQL
psql -h 192.168.1.50 -U n8n_user -d n8n -c "SELECT 1;"
# Respuesta esperada: 1

# Si no tienes los clientes instalados:
docker run --rm redis:7-alpine redis-cli -h 192.168.1.50 ping
docker run --rm postgres:15-alpine psql -h 192.168.1.50 -U n8n_user -d n8n -c "SELECT 1;"
```

---

## 🚀 Paso 5: Levantar Workers

```bash
# En VPS
cd /opt/n8n/vps

# Levantar con número de workers especificado en .env
docker compose -f docker-compose.vps.yml up -d

# O especificar replicas directamente
docker compose -f docker-compose.vps.yml up -d --scale n8n-worker=3

# Verificar que esté corriendo
docker compose -f docker-compose.vps.yml ps
```

**Salida esperada:**

```
NAME              IMAGE           STATUS
n8n-worker-1      n8nio/n8n:latest    Up 10 seconds
n8n-worker-2      n8nio/n8n:latest    Up 9 seconds
n8n-worker-3      n8nio/n8n:latest    Up 8 seconds
```

---

## 🏥 Paso 6: Verificar Salud

```bash
# En VPS
cd /opt/n8n

# Ejecutar health check
chmod +x vps/script/health-check.sh
./vps/script/health-check.sh

# Deberías ver:
✅ Docker daemon está corriendo
✅ Encontrados 3 contenedores n8n
✅ Espacio en disco OK: 45% utilizado
✅ Memoria disponible OK: 60%
✅ Carga de CPU OK: 1.2
✅ Conectividad de red OK
✅ Logs limpios
✅ SISTEMA SALUDABLE
```

---

## 📊 Paso 7: Monitoreo Inicial

```bash
# Ver logs
docker compose -f vps/docker-compose.vps.yml logs -f

# Ver estadísticas
./vps/script/maintenance.sh stats

# Ver logs de workers específicamente
./vps/script/maintenance.sh logs n8n-worker 50
```

**Logs esperados:**

```
n8n-worker-1 | Starting n8n worker
n8n-worker-1 | Listening to queue: default
n8n-worker-1 | Ready to execute workflows
```

---

## 🔄 Paso 8: Configurar Actualizaciones Automáticas (Opcional)

```bash
# En VPS - agregar a crontab
crontab -e

# Agregar esta línea para actualizar imágenes cada semana (domingo a las 2 AM)
0 2 * * 0 cd /opt/n8n && docker compose -f vps/docker-compose.vps.yml pull && docker compose -f vps/docker-compose.vps.yml up -d
```

---

## ✨ ¿Qué Sigue?

### Desde tu PC, verificar que funciona:

```bash
# Escalar a 5 workers
./script/vps-helper.sh scale 5

# Ver estado
./script/vps-helper.sh status

# Ver logs
./script/vps-helper.sh logs-workers
```

### En VPS - Mantenimiento Regular:

```bash
# Diarios
./vps/script/health-check.sh

# Semanales
./vps/script/maintenance.sh cleanup
./vps/script/maintenance.sh stats

# Mensuales
./vps/script/maintenance.sh rotate-backups
./vps/script/maintenance.sh update
```

---

## 🆘 Troubleshooting Rápido

### "Error: Cannot connect to Redis"
```bash
# Verificar conectividad
redis-cli -h 192.168.1.50 ping

# Verificar .env
cat /opt/n8n/vps/.env | grep REDIS

# Comprobar firewall en servidor de Redis
# Redis debe aceptar conexiones en puerto 6379
```

### "Error: Database connection failed"
```bash
# Verificar conectividad
psql -h 192.168.1.50 -U n8n_user -d n8n -c "SELECT 1;"

# Verificar credenciales en .env
cat /opt/n8n/vps/.env | grep DB_

# Comprobar firewall en servidor de PostgreSQL
```

### "N8N_ENCRYPTION_KEY mismatch"
```bash
# Debe ser idéntica a la del PC
# En PC, ver:
cat .env | grep N8N_ENCRYPTION_KEY

# En VPS, copiar el mismo valor:
nano vps/.env
# Pegar el valor exacto

# Reiniciar
docker compose -f vps/docker-compose.vps.yml restart
```

### "Workers no procesan trabajos"
```bash
# Ver logs
docker compose -f vps/docker-compose.vps.yml logs n8n-worker

# Verificar que está escuchando la cola
# Debe mostrar: "Listening to queue: default"

# Si no, reiniciar todo
docker compose -f vps/docker-compose.vps.yml down
docker compose -f vps/docker-compose.vps.yml up -d
```

---

## 📚 Documentación VPS

- [HEALTH_MONITORING.md](HEALTH_MONITORING.md) - Monitoreo integral
- [MAINTENANCE_GUIDE.md](MAINTENANCE_GUIDE.md) - Tareas de mantenimiento
- [SCALING_STRATEGIES.md](SCALING_STRATEGIES.md) - Cómo escalar workers
- [TROUBLESHOOTING_VPS.md](TROUBLESHOOTING_VPS.md) - Problemas avanzados

---

## 📋 Checklist Final

```
☐ Docker y Docker Compose instalados y funcionando
☐ Archivo .env configurado con valores correctos
☐ Redis externo accesible y respondiendo
☐ PostgreSQL externo accesible y respondiendo
☐ N8N_ENCRYPTION_KEY idéntica a tu máquina local
☐ docker-compose.vps.yml levantado
☐ health-check.sh mostrando ✅ SISTEMA SALUDABLE
☐ Logs sin errores críticos
☐ Desde PC: vps-helper.sh status respondiendo
☐ Workers en escucha en Redis queue
```

**¡Listo! Tu VPS está configurado y corriendo. 🚀**
