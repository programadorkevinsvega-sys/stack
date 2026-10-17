# 🆘 VPS Troubleshooting Guide

Este documento cubre problemas comunes en VPS y sus soluciones.

---

## 📋 Índice de Problemas

1. [Contenedores](#contenedores)
2. [Conectividad](#conectividad)
3. [Rendimiento](#rendimiento)
4. [Logs y Debugging](#logs-debugging)
5. [Recursos](#recursos)
6. [Actualización](#actualización)

---

## 🐳 Contenedores

### Problema: Contenedores no inician

**Síntomas:**
```
ERROR: Exited (1)
Container exits immediately after start
```

**Diagnóstico:**

```bash
# Ver estado
docker compose -f vps/docker-compose.vps.yml ps

# Ver logs
docker compose -f vps/docker-compose.vps.yml logs n8n-worker
```

**Causas Comunes:**

#### 1. Imágenes no descargadas
```bash
# Solución
docker compose -f vps/docker-compose.vps.yml pull
docker compose -f vps/docker-compose.vps.yml up -d
```

#### 2. Puertos ocupados
```bash
# Ver qué está ocupando puerto
lsof -i :5678
netstat -tulpn | grep 5678

# Solución: cambiar puerto o matar proceso
kill -9 <PID>
```

#### 3. Error de permiso en volúmenes
```bash
# Ver error específico
docker compose -f vps/docker-compose.vps.yml logs

# Solución: fix permissions
sudo chown -R 1000:1000 /opt/n8n/vps/.volumes/

# Resetear volúmenes
docker compose -f vps/docker-compose.vps.yml down -v
docker compose -f vps/docker-compose.vps.yml up -d
```

#### 4. Falta variable de entorno
```bash
# Verificar .env existe y está completo
cat /opt/n8n/vps/.env | grep -c "="
# Debería mostrar 10+

# Comparar con template
diff <(sort /opt/n8n/vps/.env.example.vps) <(sort /opt/n8n/vps/.env)

# Copiar variables faltantes
cp /opt/n8n/vps/.env.example.vps /opt/n8n/vps/.env
nano /opt/n8n/vps/.env  # Editar con valores correctos
```

---

### Problema: Worker no conecta a Redis

**Síntomas:**
```
ERROR: Cannot connect to Redis at 192.168.1.50:6379
ECONNREFUSED
```

**Diagnóstico:**

```bash
# Test directo
redis-cli -h 192.168.1.50 ping
# Debería responder PONG

# Test desde contenedor
docker compose -f vps/docker-compose.vps.yml exec n8n-worker \
  redis-cli -h 192.168.1.50 ping
```

**Causas Comunes:**

#### 1. IP de Redis incorrecta
```bash
# Verificar en .env
cat /opt/n8n/vps/.env | grep REDIS_HOST

# Ping a la IP
ping 192.168.1.50

# Cambiar si es necesario
nano /opt/n8n/vps/.env
# Editar QUEUE_BULL_REDIS_HOST=<IP_CORRECTA>

# Relajar
docker compose -f vps/docker-compose.vps.yml restart
```

#### 2. Firewall bloqueando puerto
```bash
# Verificar en servidor Redis
sudo iptables -L | grep 6379

# Si está bloqueado, abrir
sudo ufw allow 6379  # Ubuntu

# Si Redis está en host remoto:
# Verificar que listen en todas las interfaces
redis-cli CONFIG GET bind
# Debería mostrar 0.0.0.0 o IP específica (no 127.0.0.1)
```

#### 3. Redis no está corriendo
```bash
# En servidor de Redis
redis-cli ping
# Si da error, Redis no está corriendo

redis-server --daemonize yes
# O via Docker
docker run -d -p 6379:6379 redis:7-alpine
```

---

### Problema: Worker no conecta a PostgreSQL

**Síntomas:**
```
ERROR: Database connection failed
ECONNREFUSED
PostgreSQL: Database "n8n" does not exist
```

**Diagnóstico:**

```bash
# Test directo
psql -h 192.168.1.50 -U n8n_user -d n8n -c "SELECT 1;"

# Test desde contenedor
docker compose -f vps/docker-compose.vps.yml exec n8n-worker \
  psql -h 192.168.1.50 -U n8n_user -d n8n -c "SELECT 1;"
```

**Soluciones:**

#### 1. Credenciales incorrectas
```bash
# Verificar en .env
cat /opt/n8n/vps/.env | grep DB_

# Probar credenciales en servidor PostgreSQL
psql -h localhost -U n8n_user -d n8n -c "SELECT 1;"

# Si da error, verificar usuario y contraseña
```

#### 2. BD no existe
```bash
# En servidor PostgreSQL
psql -U postgres -c "CREATE DATABASE n8n;"
psql -U postgres -c "CREATE USER n8n_user WITH PASSWORD 'password';"
psql -U postgres -c "GRANT ALL ON DATABASE n8n TO n8n_user;"
```

#### 3. IP de BD incorrecta
```bash
# Verificar IP en .env
cat /opt/n8n/vps/.env | grep DB_POSTGRESDB_HOST

# Ping a la IP
ping 192.168.1.50

# Cambiar en .env si es necesario
nano /opt/n8n/vps/.env
```

---

## 🔌 Conectividad

### Problema: Sin internet en contenedor

**Síntomas:**
```
Cannot reach external APIs
curl: (6) Could not resolve host
npm install fails
```

**Diagnóstico:**

```bash
# Test desde contenedor
docker compose -f vps/docker-compose.vps.yml exec n8n-worker \
  curl -I https://google.com

docker compose -f vps/docker-compose.vps.yml exec n8n-worker \
  cat /etc/resolv.conf
```

**Soluciones:**

#### 1. DNS no configurado
```bash
# En VPS host
cat /etc/resolv.conf

# Si está vacío, agregar DNS
echo "nameserver 8.8.8.8" | sudo tee -a /etc/resolv.conf
echo "nameserver 8.8.4.4" | sudo tee -a /etc/resolv.conf

# Restart Docker
sudo systemctl restart docker
```

#### 2. Network misconfigured
```bash
# Reset Docker networks
docker network prune -f
docker compose -f vps/docker-compose.vps.yml down
docker compose -f vps/docker-compose.vps.yml up -d
```

#### 3. ISP bloqueando puertos
```bash
# Test conectividad alternativa
curl -I https://example.com:8443
nslookup google.com 8.8.8.8

# Considerar proxy si es necesario
```

---

## ⚡ Rendimiento

### Problema: Workers lentos o cuelgan

**Síntomas:**
```
Workflows executing slow
Workers unresponsive
High CPU/Memory usage
```

**Diagnóstico:**

```bash
# Ver estadísticas
./vps/script/maintenance.sh stats
./vps/script/maintenance.sh resources

# Ver logs de error
./vps/script/maintenance.sh follow n8n-worker

# Verificar procesos
top
# Tipo: P para sort por CPU
# Tipo: M para sort por memoria
```

**Soluciones:**

#### 1. Demasiados workers para la RAM
```bash
# Verificar usar total
docker stats --no-stream

# Reducir workers
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=5

# Aumentar RAM del VPS si es necesario
```

#### 2. PostgreSQL lenta
```bash
# Verificar conexiones abiertas
psql -U n8n_user -d n8n -c "SELECT COUNT(*) FROM pg_stat_activity;"

# Ver queries lentas
psql -U n8n_user -d n8n -c "SELECT query, calls, mean_time FROM pg_stat_statements ORDER BY mean_time DESC LIMIT 10;"

# Si: aumentar conexiones permitidas en PostgreSQL
nano /etc/postgresql/postgresql.conf
# max_connections = 200
```

#### 3. Redis saturado
```bash
# Verificar uso de memoria
redis-cli INFO memory

# Ver comandos lentos
redis-cli SLOWLOG GET 10

# Limpiar lista de trabajos completados
redis-cli LLEN queue:default

# Aumentar maxmemory si es necesario
redis-cli CONFIG SET maxmemory 2gb
```

---

### Problema: Memoria memory leak

**Síntomas:**
```
Worker memory keeps increasing
Docker stats shows growing memory
Eventually runs out of RAM
```

**Diagnóstico:**

```bash
# Monitorear en tiempo real
watch -n 5 'docker stats --no-stream'

# Ver logs buscando errores memory
./vps/script/maintenance.sh logs n8n-worker 500 | grep -i "memory\|leak"
```

**Soluciones:**

#### 1. Problema conocido en n8n
```bash
# Mantener versión actualizada
./vps/script/maintenance.sh update
docker compose -f vps/docker-compose.vps.yml up -d

# Check release notes
# https://docs.n8n.io/changelog/
```

#### 2. Workflow con memory leak
```bash
# Identificar workflow que consume memoria
# (monitorear qué workflows corren cuando memory sube)

# Revisar nodos en ese workflow:
- Node.js execution
- Large data transformations
- Loops sin exit condition

# Soluciones:
- Optimizar workflow
- Usar chunking para datos grandes
- Agregar limits en loops
```

---

## 📋 Logs y Debugging

### Problema: "No información en logs"

**Solución:**

```bash
# Ver todos los logs
docker compose -f vps/docker-compose.vps.yml logs

# Tail/follow en vivo
./vps/script/maintenance.sh follow n8n-worker

# Con filtro por nivel (solo errores)
docker compose -f vps/docker-compose.vps.yml logs | grep ERROR

# Últimas 200 líneas
./vps/script/maintenance.sh logs n8n-worker 200

# Exportar a archivo
docker compose -f vps/docker-compose.vps.yml logs > /tmp/n8n-logs.txt
```

### Problema: Logs muy grandes

```bash
# Limpiar logs viejos
./vps/script/maintenance.sh clean-logs 7

# Configurar logrotate
sudo nano /etc/logrotate.d/n8n

# Agregar:
/var/log/n8n/*.log {
    daily
    rotate 7
    compress
    delaycompress
    notifempty
    create 0640 root root
}
```

---

## 💾 Recursos

### Problema: Disco lleno

**Diagnóstico:**

```bash
# Ver uso total
df -h
df -h /opt/n8n

# Ver qué está ocupando
du -sh /opt/n8n/*
du -sh /var/lib/docker
```

**Soluciones (en orden):**

```bash
# 1. Limpiar logs
./vps/script/maintenance.sh clean-logs 7

# 2. Limpiar Docker
./vps/script/maintenance.sh cleanup

# 3. Rotar backups viejos
./vps/script/maintenance.sh rotate-backups

# 4. Ver qué más está usando
find /opt/n8n -type f -size +100M -exec ls -lh {} \;

# 5. Si sigue lleno - aumentar disco
# (dependiendo del proveedor)
```

---

### Problema: Sin espacio para logs

```bash
# Emergencia - truncar todos los logs
for container in $(docker ps -q); do
    truncate -s 0 $(docker inspect -f '{{.LogPath}}' $container)
done

# Luego
./vps/script/maintenance.sh clean-logs 1
```

---

## ♻️ Actualización

### Problema: Actualizar n8n falla

**Diagnóstico:**

```bash
# Ver logs de pull
docker compose -f vps/docker-compose.vps.yml pull --help

# Descargar con output
docker pull n8nio/n8n:latest
```

**Soluciones:**

#### 1. Sin acceso a Docker Hub
```bash
# Verificar conectividad
curl -I https://hub.docker.com

# Si está bloqueado, use mirror alternativo
nano /etc/docker/daemon.json

# Agregar:
{
  "registry-mirrors": [
    "https://mirror.aliyun.com",
    "https://registry.docker-cn.com"
  ]
}

sudo systemctl restart docker
```

#### 2. Imagen corrupta
```bash
# Remover imagen vieja
docker image rm n8nio/n8n:latest

# Descargar de nuevo
docker compose -f vps/docker-compose.vps.yml pull
```

#### 3. Aplicar actualización causa problemas
```bash
# Rollback a versión anterior
docker compose -f vps/docker-compose.vps.yml down

# Cambiar versión en compose
nano vps/docker-compose.vps.yml
# Cambiar: image: n8nio/n8n:latest  →  image: n8nio/n8n:1.x.x

# Levantar versión anterior
docker compose -f vps/docker-compose.vps.yml pull
docker compose -f vps/docker-compose.vps.yml up -d

# Reportar issue en GitHub
```

---

## 🆘 Último Recurso: Nuclear Option

```bash
# SOLO si nada más funciona - resetea todo
docker compose -f vps/docker-compose.vps.yml down -v
docker image prune -a -f
docker container prune -f
docker network prune -f
docker volume prune -f

# Levantar de nuevo
docker compose -f vps/docker-compose.vps.yml pull
docker compose -f vps/docker-compose.vps.yml up -d

# Verificar health
./vps/script/health-check.sh
```

**⚠️ ADVERTENCIA:** Esto elimina TODOS los volúmenes locales (no afecta BD externa)

---

## 🔗 Referencias

- [HEALTH_MONITORING.md](HEALTH_MONITORING.md) - Health check
- [MAINTENANCE_GUIDE.md](MAINTENANCE_GUIDE.md) - Mantenimiento
- [SCALING_STRATEGIES.md](SCALING_STRATEGIES.md) - Escalabilidad
- [n8n Issues](https://github.com/n8n-io/n8n/issues) - Reportes de bugs

---

## 📞 Escalation

Si nada funciona:

1. **Recopilar información:**
   ```bash
   ./vps/script/health-check.sh > health.txt
   docker compose -f vps/docker-compose.vps.yml ps >> health.txt
   docker compose -f vps/docker-compose.vps.yml logs --tail 100 >> health.txt
   ```

2. **Contactar support:**
   - GitHub Issues: https://github.com/n8n-io/n8n/issues
   - Community Forum: https://community.n8n.io
   - Discord: https://discord.n8n.io

3. **Información a proporcionar:**
   - Archivo `health.txt` (sin credenciales)
   - Versión de n8n: `docker ps | grep n8n`
   - OS: `cat /etc/os-release`
   - Pasos para reproducir

---

**¡Debugging confiado! 🔧**
