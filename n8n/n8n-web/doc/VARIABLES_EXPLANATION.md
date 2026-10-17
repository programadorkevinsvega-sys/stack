# 🔐 Variables Clave de n8n - Explicación Completa

## Variables Faltantes - Por Qué Son Importantes

### 1. 🔐 `N8N_ENCRYPTION_KEY` (Crítica)

**¿Qué es?** Clave de encriptación principal de n8n.

**¿Por qué es importante?**
- Encripta credenciales almacenadas en la BD
- Encripta variables de entorno personalizadas
- Encripta datos sensibles en workflows
- **Si cambias esta clave, NO podrás desencriptar datos antiguos**

**Ejemplo de uso:**
```bash
# Generar una clave segura de 32 caracteres
openssl rand -base64 32

# Resultado: aBc1xYz2Def3gHi4jKl5mNo6PqR7sTu8=

# Usar en .env:
N8N_ENCRYPTION_KEY=aBc1xYz2Def3gHi4jKl5mNo6PqR7sTu8=
```

**Crítico en arquitectura distribuida:**
- Local + VPS deben usar **LA MISMA CLAVE**
- Si un workflow usa credenciales encriptadas en tu máquina local, deben poder desencriptarse en VPS
- Si son diferentes, los secretos no funcionarán entre instancias

---

### 2. 📁 `DATA_FOLDER` (Importante)

**¿Qué es?** Ruta donde n8n almacena datos locales.

**¿Por qué es importante?**
- Almacena credenciales
- Guarda configuración local
- Almacena caché de integraciones
- **En Docker, esto mapea a volúmenes persistentes**

**Valores comunes:**
```bash
# En contenedor
DATA_FOLDER=/home/node/.n8n

# En servidor Linux
DATA_FOLDER=/opt/n8n-cluster/n8n-data
```

**En docker-compose.yml:**
```yaml
volumes:
  - n8n-data:/home/node/.n8n  # Mapea volumen
```

---

### 3. 🕐 `GENERIC_TIMEZONE` (Importante)

**¿Qué es?** Zona horaria para logs y ejecuciones.

**¿Por qué es importante?**
- Afecta timestamps en BD (cuándo se ejecutó cada workflow)
- Afecta logs (para debugging)
- Afecta triggers basados en tiempo
- Debe ser igual entre PC y VPS para consistencia

**Ejemplos válidos:**
```bash
# Hispanoamérica
GENERIC_TIMEZONE=America/Bogota       # Colombia
GENERIC_TIMEZONE=America/Mexico_City  # México
GENERIC_TIMEZONE=America/Buenos_Aires # Argentina
GENERIC_TIMEZONE=America/Caracas      # Venezuela

# Otros
GENERIC_TIMEZONE=Europe/Madrid        # España
GENERIC_TIMEZONE=UTC                  # Universal
```

**Problemas sin esta variable:**
```
Log: "Workflow ejecutado a 2026-03-23T15:30:00Z"  (UTC)
Esperado: 2026-03-23T10:30:00 (Bogotá, -5)
↓
Confusión en auditoría y debuggeo
```

---

### 4. 🗄️ `DB_TYPE` (Crítica en Queue Mode)

**¿Qué es?** Tipo de base de datos que usa n8n.

**¿Por qué es importante?**
- Define cómo n8n almacena datos
- En Queue Mode, DEBE ser `postgresdb`
- Sin esto, n8n no sabe cómo conectar a la BD

**Valores válidos:**
```bash
# Para Queue Mode (distribuido)
DB_TYPE=postgresdb      # PostgreSQL ✅

# Otros (no recomendados para Queue Mode)
DB_TYPE=sqlite          # SQLite (solo local, no distribuido)
DB_TYPE=mysql8          # MySQL
DB_TYPE=mariadb         # MariaDB
```

**Por qué postgresdb para Queue Mode:**
- Mejor concurrencia (múltiples workers acceden simultáneamente)
- Locks inteligentes para evitar conflictos
- Mejor performance con múltiples conexiones

---

## Acceso a Internet en Contenedores

### ✅ ¿Cómo funciona?

```
Contenedor n8n
    │
    └─→ Network Bridge (driver: bridge)
        │
        ├─→ Internet externo (APIs, webhooks)
        ├─→ Redis externo (IP:6379)
        ├─→ PostgreSQL externo (IP:5432)
        └─→ Vault externo (IP:8200)
```

### ✅ Configuración en docker-compose.yml

```yaml
services:
  n8n-web:
    networks:
      - default

networks:
  default:
    name: stack-network
    driver: bridge  # ← Permite acceso a internet
```

**Por defecto:**
- Bridge network conecta contenedores con host
- Host tiene acceso a internet
- Contenedores heredan esa conectividad
- **NO requiere configuración especial**

### ✅ ¿Qué puede hacer el contenedor?

```bash
# Dentro del contenedor n8n:

# 1. Conectar a APIs externas
curl https://api.github.com/repos

# 2. Conectar a servicios externos
redis-cli -h 192.168.1.100 ping

# 3. Descargar paquetes
npm install @sendgrid/mail

# 4. Resolver DNS
nslookup google.com

# ✅ TODO FUNCIONA SIN CONFIGURACIÓN ESPECIAL
```

### ⚠️ Si NO tuviera acceso a internet

```
Error: ECONNREFUSED xxxxx
Error: getaddrinfo ENOTFOUND api.example.com
Error: Cannot fetch external data
```

---

## Resumen: Variables Clave

| Variable | Ubicación | Papel | Crítica |
|----------|-----------|-------|---------|
| `N8N_ENCRYPTION_KEY` | docker-compose | Encripta credenciales | 🔴 SÍ |
| `DATA_FOLDER` | docker-compose | Almacena datos | 🟡 Importante |
| `GENERIC_TIMEZONE` | docker-compose | Zona horaria logs | 🟡 Importante |
| `DB_TYPE` | docker-compose | Conexión a BD | 🔴 SÍ (Queue) |
| `EXECUTIONS_MODE` | docker-compose | Mode: queue/regular | 🔴 SÍ |

---

## Checklist: Antes de Iniciar

```bash
# 1. ¿N8N_ENCRYPTION_KEY está definida?
grep N8N_ENCRYPTION_KEY .env

# 2. ¿Ambas instancias usan la MISMA KEY?
# PC: cat .env | grep N8N_ENCRYPTION_KEY
# VPS: ssh vps "cat .env | grep N8N_ENCRYPTION_KEY"
# ↓ Deben ser IGUALES

# 3. ¿DATA_FOLDER existe y tiene permisos?
ls -la /opt/n8n-cluster/n8n-data

# 4. ¿GENERIC_TIMEZONE es válido?
grep GENERIC_TIMEZONE .env

# 5. ¿DB_TYPE=postgresdb?
grep DB_TYPE .env

# 6. ¿Contenedores tienen acceso a internet?
docker compose exec n8n-web curl https://google.com
# Debe responder: código 200 u HTML de Google
```

---

## Generando Claves Seguras

```bash
# Opción 1: OpenSSL (recomendado)
openssl rand -base64 32

# Opción 2: Python
python3 -c "import secrets; print(secrets.token_urlsafe(32))"

# Opción 3: Linux
head -c 32 /dev/urandom | base64

# Resultado característico:
# x7P2zK9mL4wQ8vN3jH5dF6gY2bRtC1sU=
```

Usa el resultado en .env:
```bash
N8N_ENCRYPTION_KEY=x7P2zK9mL4wQ8vN3jH5dF6gY2bRtC1sU=
```

---

## Guía Rápida de Setup

```bash
# 1. Generar clave
KEY=$(openssl rand -base64 32)

# 2. Editar .env
echo "N8N_ENCRYPTION_KEY=$KEY" >> .env

# 3. Verificar variables
cat .env | grep -E "N8N_ENCRYPTION_KEY|DATA_FOLDER|GENERIC_TIMEZONE|DB_TYPE"

# 4. Iniciar
docker compose up -d

# 5. Verificar conectividad
docker compose logs n8n-web | grep -i "connected"
```
