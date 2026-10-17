# ⚡ INICIO RÁPIDO: n8n Local (5 minutos)

## 🎯 Objetivo
Tener n8n corriendo en tu máquina local con 2 workers, conectados a servicios externos (Redis, PostgreSQL).

---

## 📋 Pre-requisitos

- [ ] Redis externo accesible (IP:6379)
- [ ] PostgreSQL externo accesible (IP:5432)
- [ ] Docker instalado en tu PC
- [ ] Docker Compose v2.0+
- [ ] Variables .env configuradas

**Verifica conectividad PRIMERO:**
```bash
redis-cli -h 192.168.1.100 ping          # Debe responder: PONG
psql -h 192.168.1.100 -U n8n_user -d n8n -c "SELECT 1"  # Debe responder: 1
```

---

## 🚀 Paso 1: Copiar Archivo de Variables (1 minuto)

```bash
cd /opt/n8n
cp .env.example .env
```

---

## 🔐 Paso 2: Editar .env (2 minutos)

```bash
nano .env
```

docker network connect proxy-network redis

docker network connect proxy-network postgres

docker network connect proxy-network minio

docker network connect proxy-network vault

**Variables CRÍTICAS que editar:**

```env
# Redis (EXTERNO)
QUEUE_BULL_REDIS_HOST=192.168.1.100
QUEUE_BULL_REDIS_PORT=6379

# PostgreSQL (EXTERNO)
DB_POSTGRESDB_HOST=192.168.1.100
DB_POSTGRESDB_PORT=5432
DB_POSTGRESDB_USER=n8n_user
DB_POSTGRESDB_PASSWORD=tu_password_muy_seguro
DB_POSTGRESDB_DATABASE=n8n

# Encriptación
N8N_ENCRYPTION_KEY=generar_con_openssl_rand_-base64_32

# Acceso
N8N_HOST=localhost
N8N_PORT=5678

# Zona horaria (opcional pero recomendado)
GENERIC_TIMEZONE=America/Bogota
```

---

## ✅ Paso 3: Verificar Configuración (1 minuto)

```bash
chmod +x script/verify-config.sh
./script/verify-config.sh
```

**Esperado:** `✅ All checks passed`

---

## 🚀 Paso 4: Levantar n8n (1 minuto)

```bash
docker compose up -d
docker compose ps
```

**Esperado:**
```
n8n-web         Up (healthy)
n8n-worker-1    Up
n8n-worker-2    Up
```

---

## 🌐 Paso 5: Acceder a n8n

**Abre tu navegador:**
```
http://localhost:5678
```

**¡Listo! 🎉**

---

## 📚 Documentación por Tema

- [ARCHITECTURE_SIMPLE.md](ARCHITECTURE_SIMPLE.md) - Cómo funciona
- [VARIABLES_EXPLANATION.md](VARIABLES_EXPLANATION.md) - Qué significa cada variable
- [SCRIPT_LOCATION.md](SCRIPT_LOCATION.md) - Cuándo usar cada script
