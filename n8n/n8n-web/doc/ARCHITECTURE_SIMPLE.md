# 🏗️ Arquitectura Simplificada - Local Only

## Los 2 Lugares Clave

### 1. 🖥️ SERVICIOS EXTERNOS (El Core)

**Dónde está:** Otro servidor/Docker (puede ser mismo host, pero Docker diferente)

**Qué corre aquí:**
- `Redis:6379` - Cola de trabajos
- `PostgreSQL:5432` - Base de datos principal
- `Vault:8200` - Almacén de secretos (opcional)

**Quién se conecta:** Tu máquina local (y opcionalmente VPS si lo tienes)

**Acceso:**
```bash
# Desde tu PC:
redis-cli -h 192.168.1.100 ping          # ✅ Debe responder PONG
psql -h 192.168.1.100 -U n8n_user ...    # ✅ Debe responder 1
```

**¿Por qué no en docker-compose.yml?**
- ✅ Porque YA EXISTEN en otro lado
- ✅ Para no crear duplicados
- ✅ Para que tu PC se conecte a ellos

---

### 2. 💻 LOCAL (Tu Máquina)

**Ubicación:** Tu máquina local

**Qué corre aquí:**
```bash
docker-compose.yml  →  n8n-web (interfaz) + 2 n8n-workers (procesamiento local)
```

**Qué no corre aquí:**
```
❌ Redis         (en servidor EXTERNO)
❌ PostgreSQL    (en servidor EXTERNO)
❌ Vault         (en servidor EXTERNO)
```

**Scripts que ejecutas en tu máquina:**
```bash
./script/verify-config.sh          # ✅ Verifica antes de iniciar
docker compose up -d               # ✅ Levanta n8n local
./script/backup.sh                 # ✅ Hace backup de BD
docker compose logs -f             # ✅ Ve logs locales
```

**Acceso:**
- 🌐 http://localhost:5678 (interfaz gráfica)
- 📡 Conecta a Redis externo en 192.168.1.100:6379
- 📄 Conecta a PostgreSQL externo en 192.168.1.100:5432

---

## 📋 Componentes (Local Only)

```
COMPONENTE         UBICACIÓN              LEVANTADO CON
────────────────────────────────────────────────────────

Redis              Servidor Externo       (No por docker-compose)
PostgreSQL         Servidor Externo       (No por docker-compose)  
Vault              Servidor Externo       (No por docker-compose)

n8n-web            Local (ti)          docker-compose.yml
n8n-worker x2      Local (ti)          docker-compose.yml
```

---

## ⚡ Flujo de Datos (Local)

```
1. Tú escribes un workflow en http://localhost:5678
   ↓
2. n8n-web envía el trabajo a Redis (EXTERNO)
   ↓
3. Workers locales (tu PC) escuchan y se toman los trabajos
   ↓
4. Ejecutan y guardan resultado en PostgreSQL (EXTERNO)
   ↓
5. Verificas resultado en http://localhost:5678
```

---

## 🔌 Conectividad (Local)

```
            Tu Máquina Local
                 │
    ┌────────────┼────────────┐
    │            │            │
    ▼            ▼            ▼
 [Redis]  [PostgreSQL]  [Vault]
 (Externo)  (Externo)  (Externo)
```

---

## 🚨 Lo Más Importante

### ✅ Esto DEBE estar funcionando PRIMERO:

```bash
# 1. Redis accesible
redis-cli -h 192.168.1.100 ping
# PONG ✅

# 2. PostgreSQL accesible
psql -h 192.168.1.100 -U n8n_user -d n8n -c "SELECT 1"
# 1 ✅

# 3. Vault accesible (opcional)
curl http://192.168.1.100:8200/ui/
# [HTML] ✅
```

### ✅ ENTONCES levanta tu PC:

```bash
# En tu PC
docker compose up -d
```

### ⚠️ Si lo haces al revés:

```
docker compose up -d  →  "Error: Cannot connect to Redis"
                         ↑
                    Porque Redis no existe aún
```

---

## 📊 Ejemplo Real (Local)

### IPs Reales

```
Servicios Externos:  192.168.1.50   (Servidor con Redis, PostgreSQL, Vault)
Local:            192.168.1.100  (Tu máquina con n8n-web + 2 workers)
```

### Configuración Necesaria

**Local (.env):**
```env
N8N_HOST=192.168.1.100              # Tu PC es accesible en esta IP
QUEUE_BULL_REDIS_HOST=192.168.1.50  # Redis está aquí
DB_POSTGRESDB_HOST=192.168.1.50     # PostgreSQL está aquí
VAULT_ADDR=http://192.168.1.50:8200 # Vault está aquí
```

**VPS Remoto (.env):**
```env
QUEUE_BULL_REDIS_HOST=192.168.1.50  # Same Redis
DB_POSTGRESDB_HOST=192.168.1.50     # Same PostgreSQL
VAULT_ADDR=http://192.168.1.50:8200 # Same Vault
```

### Flujo:

```
1. Tú abres: http://192.168.1.100:5678
   ↓
2. Creas workflow en tu máquina local (n8n-web)
   ↓
3. Se envía a: 192.168.1.50:6379 (Redis)
   ↓
4. Procesa en:
   ├─ Workers en 192.168.1.100 (Local)  ← 2 workers
   ├─ Workers en 45.32.100.5 (VPS)         ← 3+ workers
   ↓
5. Resultado guardado en: 192.168.1.50:5432 (PostgreSQL)
   ↓
6. Verificas en: http://192.168.1.100:5678
```

---

## 🎯 Resumen: Para Recordar

| Pregunta | Respuesta |
|----------|-----------|
| ¿Dónde está Redis? | **Externo** (otro servidor/Docker) |
| ¿Dónde está PostgreSQL? | **Externo** (otro servidor/Docker) |
| ¿Dónde está n8n-web? | **Local** |
| ¿Dónde están primeros 2 workers? | **Local** |
| ¿Dónde están más workers? | **VPS Remoto** |
| ¿Quién levanta Redis? | **Admin** (fuera de este repo) |
| ¿Quién levanta PostgreSQL? | **Admin** (fuera de este repo) |
| ¿Quién levanta n8n-web? | **Tú en tu máquina local** (`docker compose up`) |
| ¿Quién levanta workers VPS? | **Tú en VPS** (`docker compose -f docker-compose.vps.yml up`) |
| ¿Quien gestiona VPS desde PC? | **Tú con** `./vps-helper.sh` |
| ¿Dónde ejecutar verify-config.sh? | **Local** |
| ¿Dónde ejecutar backup.sh? | **Local o VPS** (tu elección) |
| ¿Dónde ejecutar vps-helper.sh? | **Local** (gestiona VPS remoto) |

---

## 📖 Documentos Relacionados

- [SCRIPT_LOCATION.md](SCRIPT_LOCATION.md) - Dónde ejecutar cada script
- [VARIABLES_EXPLANATION.md](VARIABLES_EXPLANATION.md) - Qué significa cada variable
- [FILE_STRUCTURE.md](FILE_STRUCTURE.md) - Estructura de archivos
- [README.md](README.md) - Guía principal
- [QUICK_START.md](QUICK_START.md) - Inicio rápido (5 minutos)
