# 📍 Dónde Ejecutar Scripts (Local Only)

## 🎯 Ubicación de Componentes - LOCAL

```
Tu Máquina Local
    ├── n8n-web      ✅ Levantado por docker-compose.yml
    ├── n8n-worker   ✅ Levantado por docker-compose.yml (x2)
    │
    ├── Redis        ❌ Externo (otro servidor)
    ├── PostgreSQL   ❌ Externo (otro servidor)
    └── Vault        ❌ Externo (otro servidor)
```

---

## 📍 Scripts de tu Máquina Local

### 1️⃣ `verify-config.sh` - Verificación Pre-Deploy

**Ejecutar en:** Tu máquina local

```bash
cd /opt/n8n
chmod +x script/verify-config.sh
./script/verify-config.sh
```

**Qué verifica:**
- ✅ Variables de entorno definidas
- ✅ Redis accesible
- ✅ PostgreSQL accesible
- ✅ Docker instalado
- ✅ Clave de encriptación

**Cuándo ejecutar:**
- ✅ ANTES de `docker compose up -d`
- ✅ Cuando cambias `.env`
- ✅ Para diagnóstico de problemas

---

### 2️⃣ `backup.sh` - Backup de Base de Datos

**Ejecutar en:** Tu máquina local

```bash
cd /opt/n8n
chmod +x script/backup.sh
./script/backup.sh
```

**Qué hace:**
- Hace dump de PostgreSQL
- Comprime con gzip
- Guarda en `./backups/`

**Manual:**
```bash
./script/backup.sh
```

**Automático (cron):**
```bash
crontab -e
# Agregar:
0 2 * * * /opt/n8n/script/backup.sh >> /opt/n8n/backups/backup.log 2>&1
```

**Diariamente a las 2 AM ✅**

---

### 3️⃣ `QUICK_REFERENCE.sh` - Cheat Sheet Visual

**Ejecutar en:** Tu máquina local

```bash
chmod +x script/QUICK_REFERENCE.sh
./script/QUICK_REFERENCE.sh
```

**Qué muestra:**
- Arquitectura visual
- Comandos más usados
- Checklist previo
- Cosas que NO debes hacer

---

## 🚀 Scripts de VPS

Para información sobre scripts del VPS, ver:
👉 [vps/doc/README.md](../vps/doc/README.md)

Scripts del VPS:
- `vps/script/health-check.sh` - Monitoreo VPS
- `vps/script/maintenance.sh` - Mantenimiento VPS

Ejecutados EN el VPS o via SSH.

---

## 📊 Resumen Rápido

| Script | Ubicación | Ejecutar en | Propósito |
|--------|-----------|-------------|----------|
| verify-config.sh | script/ | Local | Validar antes de levantar |
| backup.sh | script/ | Local | Backup manual o cron |
| QUICK_REFERENCE.sh | script/ | Local | Cheat sheet visual |
| health-check.sh | vps/script/ | VPS | Monitoreo VPS |
| maintenance.sh | vps/script/ | VPS | Mantenimiento VPS |

