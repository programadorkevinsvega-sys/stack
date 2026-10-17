# 🚀 PORTABILIDAD RÁPIDA - Stack Completo de n8n

## ⚡ Versión Ultra-Rápida

```bash
# 🔴 PC ORIGEN
tar -czf n8n-backup.tar.gz database/

# 🟢 PC DESTINO
tar xzf n8n-backup.tar.gz
cd database/
./fix-all-permissions.sh
docker compose -f docker-compose.*.yml up -d
```

---

## 📋 Servicios Portables

| Servicio | Puerto | Volumen | Estado |
|----------|--------|---------|--------|
| **PostgreSQL** | 5432 | `volumes/postgres/` | ✅ Listo |
| **Redis** | 6379 | `volumes/redis/` | ✅ Listo |
| **MySQL** | 3306 | `volumes/mysql/` | ✅ Listo |
| **MongoDB** | 27017 | `volumes/mongodb/` | ✅ Listo |
| **RabbitMQ** | 5672 | `volumes/rabbitmq/` | ✅ Listo |
| **Kafka** | 9092 | `volumes/kafka/` | ✅ Listo |
| **MinIO** | 9000 | `volumes/minio/` | ✅ Listo |
| **Vault** | 8500 | `volumes/vault/` | ✅ Listo |

---

## 🎯 3 Formas de Hacerlo

### 1️⃣ Copiar Carpeta Directamente
```bash
# Origen
cp -r database/ /otro-pc:/ruta/destino/

# Destino
cd database/
./fix-all-permissions.sh
docker compose -f docker-compose.postgres.yml up -d  # etc
```

### 2️⃣ Script Automático de Empaque
```bash
# Origen
./export-all-data.sh
# Genera: n8n-database-backup-20260403-141537.tar.gz

# Destino
tar xzf n8n-database-backup-*.tar.gz
cd database/
./fix-all-permissions.sh
docker compose -f docker-compose.*.yml up -d
```

### 3️⃣ Empacar Manual
```bash
# Origen
tar -czf n8n-database.tar.gz database/

# Destino
tar xzf n8n-database.tar.gz
cd database/
./fix-all-permissions.sh
docker compose -f docker-compose.*.yml up -d
```

---

## 🔐 Lo Que Se Incluye

✅ TODOS los datos de BD:
- PostgreSQL (23 MB)
- Redis (1-10 MB)
- MySQL (20-50 MB)
- MongoDB (10-30 MB)
- RabbitMQ (5-20 MB)
- Kafka (20-100 MB)
- MinIO (100 MB - 1 GB)
- Vault (5-10 MB)

✅ TODOS los certificados SSL:
- PostgreSQL SSL
- Redis TLS

✅ TODAS las configuraciones de docker-compose

---

## 📚 Documentación Completa

Consulta `PORTABILIDAD_COMPLETA.md` para:
- ⚙️ Solución de problemas
- 🔧 Cambiar puertos
- 🔐 Problemas de permisos
- 📊 Tamaños de datos
- 🎯 Mejores prácticas

---

## ✨ Garantías

✅ **Sin perder datos** - Todo está en volúmenes locales  
✅ **Sin problemas de permisos** - Script automático  
✅ **Sin certificados rotos** - Todo incluido  
✅ **Sin reconfiguración** - Copia y va  
✅ **Sin reinstalación** - Listo para producción  

---

**¡Listo!** Ahora puedes llevar TODA tu stack de n8n a cualquier PC sin problemas. 🚀
