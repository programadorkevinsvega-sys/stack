# 🤖 n8n - Plataforma de Automatización

Directorio raíz con configuraciones Docker para ejecutar **n8n** (plataforma de automatización de workflows) en diferentes escenarios: desde desarrollo local hasta producción escalable.

## 📁 Estructura

```
n8n/
├── n8n-web/                    # Configuración estándar escalable (RECOMENDADO)
│   ├── docker-compose.yml      # Stack con n8n web + 2 workers
│   ├── .env.example            # Ejemplo de variables de entorno
│   ├── doc/                    # Documentación adicional
│   └── script/                 # Scripts de utilidad
│
├── n8n-all-basico/             # Configuración básica simplificada
│   ├── docker-compose-basico.yml
│   ├── .env
│   └── script/
│
├── n8n-workerflow-vps/         # Configuración VPS específica
│   ├── docker-compose.vps.yml
│   └── doc/
│
└── proxy/                      # Proxy reverso (Nginx o Traefik)
    ├── docker-compose.nginx.yml
    ├── docker-compose.traefik.yml
    ├── nginx.conf
    └── README.md
```

---

## 🎯 ¿Cuál Configuración Usar?

### 1️⃣ **n8n-web** - Modo Escalable (⭐ RECOMENDADO)

**Ideal para:** Producción, múltiples workflows, alta concurrencia

**Características:**
- ✅ Modo Queue (ejecuciones en workers)
- ✅ Redis para colas de trabajo
- ✅ PostgreSQL para base de datos
- ✅ 2 workers incluidos (escalables)
- ✅ Webhooks optimizados
- ✅ Auto-recuperación

**Componentes:**
- `n8n-web`: Interfaz y orquestador (puerto 5678)
- `n8n-worker-1` y `n8n-worker-2`: Ejecutan workflows
- `redis`: Cola de tareas
- `postgres`: Base de datos compartida

**Requisitos:**
- Redis accesible (interno o externo)
- PostgreSQL accesible (interno o externo)
- 6 vCPU / 12 GB RAM recomendado

**Inicio rápido:**
```bash
cd n8n-web
cp .env.example .env
# Editar .env con tus valores
docker compose up -d
```

**Ventajas:**
- Escalable horizontalmente (añade más workers)
- Aislamiento entre n8n-web y workers
- Alta disponibilidad
- Mejor para carga pesada

---

### 2️⃣ **n8n-all-basico** - Configuración Simple

**Ideal para:** Desarrollo, testing, entornos pequeños

**Características:**
- ✅ Instalación simplificada
- ✅ Menos dependencias
- ✅ Bajo consumo de recursos
- ✅ Configuración manual mínima

**Componentes:**
- n8n todo en uno (web + ejecutor)
- Redis (opcional)

**Requisitos:**
- 2 vCPU / 4 GB RAM mínimo

**Inicio rápido:**
```bash
cd n8n-all-basico
docker compose -f docker-compose-basico.yml up -d
```

**Ventajas:**
- Rápido de configurar
- Perfecto para pruebas
- Bajo footprint de recursos

---

### 3️⃣ **n8n-workerflow-vps** - VPS Específica

**Ideal para:** Servidores VPS dedicados

**Características:**
- ✅ Optimizado para VPS
- ✅ Configuración production-ready
- ✅ Hardening de seguridad

**Requisitos:**
- Servidor VPS con IP pública
- Mínimo 4 vCPU / 8 GB RAM

**Inicio rápido:**
```bash
cd n8n-workerflow-vps
docker compose -f docker-compose.vps.yml up -d
```

---

## 🌐 Proxy Reverso (Nginx / Traefik)

Ambas configuraciones requieren un proxy reverso para exponer n8n a internet de forma segura.

### Nginx (Recomendado para principiantes)
```bash
cd proxy
docker compose -f docker-compose.nginx.yml up -d
```
- Ligero y rápido
- Configuración simple
- Ideal si solo necesitas n8n

### Traefik (Avanzado)
```bash
cd proxy
docker compose -f docker-compose.traefik.yml up -d
```
- Auto-descubrimiento de servicios
- HTTPS automático (Let's Encrypt)
- Dashboard web
- Perfecto para múltiples servicios

**Ver:** [proxy/README.md](proxy/README.md) para configuración detallada

---

## 📊 Arquitectura de Red

```
INTERNET (80, 443)
    ↓
[Proxy Reverso: Nginx/Traefik]
    ↓ (privada)
[n8n-web] (5678)
    ├─→ [Redis] (6379)
    ├─→ [PostgreSQL] (5432)
    └─→ [Workers] (escalables)
```

---

## 🚀 Flujo de Inicio Rápido (n8n-web)

### Paso 1: Pre-requisitos
```bash
# Redis disponible
redis-cli -h <REDIS_HOST> ping

# PostgreSQL disponible
psql -h <PG_HOST> -p 5432 -U postgres -d n8n -c "SELECT 1"
```

### Paso 2: Configurar Variables
```bash
cd n8n-web
cp .env.example .env
nano .env  # Editar con tus valores:
# - REDIS_HOST, REDIS_PASSWORD
# - DB_POSTGRESDB_HOST, DB_POSTGRESDB_PASSWORD
# - N8N_HOST (dominio público)
# - JWT_SECRET (generar con: openssl rand -base64 32)
```

### Paso 3: Levantar Stack
```bash
docker compose up -d
docker compose logs -f n8n-web  # Ver logs
```

### Paso 4: Verificar
```bash
# Acceso local
curl -I http://localhost:5678

# Ver estado de workers
docker compose ps
```

### Paso 5: Verificar Acceso en Línea
```bash
curl -I https://tu-dominio.com  # Debe responder con 200
```

---

## 🔧 Scripts de Utilidad

### n8n-web/script/

- **`QUICK_REFERENCE.sh`** - Comandos rápidos de consult
- **`backup.sh`** - Backup de datos y configuración
- **`verify-config.sh`** - Verifica que todo esté configurado correctamente
- **`vps-helper.sh`** - Ayuda para VPS
- **`tune_kernel.sh`** - Optimización del kernel Linux

**Uso:**
```bash
cd n8n-web/script
bash backup.sh          # Hacer backup
bash verify-config.sh   # Verificar configuración
```

---

## 📋 Variables de Entorno Críticas

### Modo Queue (n8n-web)
```env
EXECUTIONS_MODE=queue              # Queue mode para escalabilidad
QUEUE_BULL_REDIS_HOST=redis        # Host de Redis
QUEUE_BULL_REDIS_PASSWORD=secret   # Contraseña Redis
DB_POSTGRESDB_HOST=postgres        # Host PostgreSQL
DB_POSTGRESDB_PASSWORD=dbpassword  # Contraseña DB
N8N_HOST=tu-dominio.com            # Dominio público
JWT_SECRET={32-char-random}        # Generar: openssl rand -base64 32
```

### Base
```env
TZ=America/Bogota
PUID=1000
PGID=1000
```

Ver [n8n-web/doc/VARIABLES_EXPLANATION.md](n8n-web/doc/VARIABLES_EXPLANATION.md) para documentación completa.

---

## 🔐 Seguridad

### SSL/TLS
- Cada configuración incluye certificados SSL
- Rutas: `ssl/postgres/`, `ssl/redis/`
- Ver [n8n-web/doc/SSL_SETUP.md](n8n-web/doc/SSL_SETUP.md)

### Acceso
- PostgreSQL protegido con contraseña
- Redis protegido (AUTH)
- n8n detrás de proxy reverso
- Firewall con solo puertos 80/443 públicos

### Contraseñas
```bash
# Generar contraseña aleatoria segura
openssl rand -base64 32

# Generar JWT_SECRET
openssl rand -base64 32
```

---

## 📈 Escalado (n8n-web)

### Agregar Más Workers
```yaml
# En docker-compose.yml, duplicar:
n8n-worker-3:
  <<: *n8n-worker
  container_name: n8n-worker-3
  
n8n-worker-4:
  <<: *n8n-worker
  container_name: n8n-worker-4
```

```bash
docker compose up -d n8n-worker-3 n8n-worker-4
```

### Monitoreo de Colas
```bash
# Conectar a Redis
redis-cli -h <REDIS_HOST>

# Ver estadísticas de colas
INFO stats

# Listar clientes conectados
CLIENT LIST
```

---

## 🐛 Troubleshooting

### n8n no accesible
```bash
# 1. Verificar que está corriendo
docker compose ps

# 2. Ver logs
docker compose logs -f n8n-web

# 3. Verificar conectividad (dentro de contenedor)
docker exec n8n-web curl -I http://localhost:5678

# 4. Verificar firewall
sudo ufw status
```

### Workers no conectan
```bash
# 1. Verificar Redis
redis-cli -h <REDIS_HOST> ping  # Debe responder PONG

# 2. Ver logs de worker
docker compose logs -f n8n-worker-1

# 3. Verificar variables en .env
cat .env | grep QUEUE_BULL
```

### PostgreSQL no accesible
```bash
# 1. Verificar conectividad
psql -h <HOST> -U postgres -c "SELECT 1"

# 2. Ver logs de postgres
docker compose logs postgres

# 3. Verificar port forwarding (si está en otro servidor)
telnet <HOST> 5432
```

### Proxy no redirige correctamente
```bash
# 1. Ver logs del proxy
docker compose logs -f proxy  # nginx/traefik

# 2. Verificar DNS
nslookup tu-dominio.com

# 3. Verificar certificado SSL
openssl s_client -connect tu-dominio.com:443
```

---

## 📚 Documentación Adicional

- [Arquitectura](doc/ARCHITECTURE.md)
- [Setup Flow](doc/SETUP_FLOW.md)
- [Creación de Networks](doc/CREATE_NETWORK.md)
- [n8n oficial docs](https://docs.n8n.io/)

---

## 🔄 Comandos Útiles

```bash
# Ver estado de todos los servicios
docker compose ps

# Ver logs en tiempo real (tema: n8n-web)
docker compose logs -f n8n-web

# Reiniciar n8n-web sin perder datos
docker compose restart n8n-web

# Exportar logs a archivo
docker compose logs > logs.txt

# Acceso shell en contenedor
docker exec -it n8n-web bash

# Listar volúmenes
docker volume ls | grep n8n

# Limpiar (⚠️ elimina datos)
docker compose down -v

# Backup de datos
docker exec n8n-web tar czf /tmp/backup.tar.gz /home/node/.n8n
docker cp n8n-web:/tmp/backup.tar.gz ./backup.tar.gz
```

---

## 📞 Soporte

- **Docs Oficiales:** https://docs.n8n.io/
- **Community:** https://community.n8n.io/
- **Issues:** https://github.com/n8n-io/n8n/issues
- **Documentación local:** Ver carpeta `doc/`

---

## 📝 Changelog

- **v1.0** (2026-04-03): Documentación inicial
  - n8n-web (modo escalable)
  - n8n-all-basico (modo simple)
  - n8n-workerflow-vps (VPS)
  - Proxy reverso (Nginx + Traefik)

---

**Última actualización:** 2026-04-03  
**Compatible con:** n8n v1.0+, Docker 20.10+, Docker Compose 2.0+
