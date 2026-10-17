# 📦 PORTABILIDAD COMPLETA - Stack Completo de Bases de Datos

## ✅ Estado Actual

**TODOS los servicios están configurados para ser COMPLETAMENTE PORTÁTILES:**

- ✓ PostgreSQL con SSL
- ✓ Redis con TLS
- ✓ MySQL
- ✓ MongoDB
- ✓ RabbitMQ
- ✓ Kafka
- ✓ MinIO
- ✓ Vault
- ✓ Certificados SSL incluidos
- ✓ Datos en volúmenes locales (`./volumes/`)

## 🚀 OPCIÓN 1: Copiar Solo la Carpeta `database/`

### En el PC origen:
```bash
# Simplemente todos tus datos están en ./volumes/
# Puedes comprimir la carpeta completa:
cd ..  # Salir de database/
tar -czf n8n-database.tar.gz database/

# O simplemente copiar sin comprimir:
cp -r database/ /ruta/destino/
```

### En el PC destino:
```bash
# 1. Extraer los datos
tar xzf n8n-database.tar.gz
# O si lo copiaste directamente, ya está

# 2. Entrar al directorio
cd database/

# 3. Arreglar permisos para todos los servicios
./fix-all-permissions.sh

# 4. Levantar los servicios necesarios
docker compose -f docker-compose.postgres.yml up -d
docker compose -f docker-compose.redis.yml up -d
docker compose -f docker-compose.mysql.yml up -d
docker compose -f docker-compose.mongodb.yml up -d
docker compose -f docker-compose.rabbitmq.yml up -d
docker compose -f docker-compose.kafka.yml up -d
docker compose -f docker-compose.minio.yml up -d
docker compose -f docker-compose.vault.yml up -d

# 5. Verificar que todo funciona
docker ps  # Ver contenedores corriendo
```

## 🚀 OPCIÓN 2: Script Automático de Exportación

### En el PC origen:
```bash
# Exportar todo automáticamente
./export-all-data.sh

# Esto crea un archivo como:
# n8n-database-backup-20260403-141537.tar.gz
```

### En el PC destino:
```bash
# Descomprimir
tar xzf n8n-database-backup-20260403-141537.tar.gz
cd database/

# Restaurar permisos y levantar
./fix-all-permissions.sh

# Levantar todos los servicios en paralelo
docker compose \
  -f docker-compose.postgres.yml \
  -f docker-compose.redis.yml \
  -f docker-compose.mysql.yml \
  -f docker-compose.mongodb.yml \
  -f docker-compose.rabbitmq.yml \
  -f docker-compose.kafka.yml \
  -f docker-compose.minio.yml \
  -f docker-compose.vault.yml \
  up -d
```

## 📊 Estructura de Directorio

```
database/
├── docker-compose.postgres.yml
├── docker-compose.redis.yml
├── docker-compose.mysql.yml
├── docker-compose.mongodb.yml
├── docker-compose.rabbitmq.yml
├── docker-compose.kafka.yml
├── docker-compose.minio.yml
├── docker-compose.vault.yml
├── fix-all-permissions.sh        ← NECESARIO en nuevo PC
├── export-all-data.sh            ← Para empacar datos
├── PORTABILIDAD_COMPLETA.md      ← Este archivo
├── ssl/
│   ├── postgres/
│   ├── redis/
│   └── ...
└── volumes/                      ← TODOS TUS DATOS
    ├── postgres/data/
    ├── postgres/wal/
    ├── redis/data/
    ├── mysql/data/
    ├── mongodb/data/
    ├── rabbitmq/data/
    ├── kafka/data/
    ├── vault/data/
    └── minio/data/
```

## ⚠️ Notas Críticas

### Permisos
- **Cada servicio tiene un UID diferente** (999 para la mayoría, 1000 para Kafka/MinIO, 100 para Vault)
- El script `fix-all-permissions.sh` arregla todo automáticamente
- Si no lo ejecutas en el nuevo PC: **permission denied errors**

### Tamaño Aproximado
```
postgres/data/   ≈ 23 MB
postgres/wal/    ≈ 17 MB
redis/data/      ≈ 1-10 MB
mysql/data/      ≈ 20-50 MB
mongodb/data/    ≈ 10-30 MB
rabbitmq/data/   ≈ 5-20 MB
kafka/data/      ≈ 20-100 MB
vault/data/      ≈ 5-10 MB
minio/data/      ≈ 100 MB - 1 GB (depende de archivos)
─────────────────────────
TOTAL            ≈ 200 MB - 2 GB (depende de datos)
```

## ✨ Ventajas

1. ✅ **Portabilidad total** - Lleva TODO a cualquier PC
2. ✅ **Sin reinstalar** - No necesitas recrear bases de datos
3. ✅ **Sin perder datos** - Todo está en volúmenes locales
4. ✅ **SSL funciona** - Certificados incluidos
5. ✅ **Configuración preservada** - Variables de entorno en docker-compose
6. ✅ **Script automático** - `fix-all-permissions.sh` resuelve problemas de permisos

## 🔧 Si algo Falla

### Error: "permission denied"
```bash
# Ejecutar de nuevo el fix-permissions
./fix-all-permissions.sh
```

### Error: "port already in use"
```bash
# Cambiar puertos en docker-compose.*.yml
# Busca líneas como:
# ports:
#   - "127.0.0.1:5432:5432"
# Cambia 5432 a otro puerto disponible
```

### Error: "database is not empty"
```bash
# Los datos ya existen, eso es lo esperado
# Si quieres borrar y empezar de cero:
rm -rf volumes/postgres/data/*
rm -rf volumes/postgres/wal/*
# Luego levantar de nuevo
```

## 📚 Documentación Individual

Cada servicio tiene su propia documentación en:
- `database/PORTABILIDAD.md` - PostgreSQL específicamente
- Archivos individuales en cada docker-compose.*.yml

## 🎯 Resumen

```bash
# PC ORIGEN - 3 comandos:
cd database/
tar -czf ../n8n-backup.tar.gz .
# Copiar n8n-backup.tar.gz a otro PC

# PC DESTINO - 4 comandos:
tar xzf n8n-backup.tar.gz
cd database/
./fix-all-permissions.sh
docker compose -f docker-compose.*.yml up -d

# ¡LISTO! Sin perder datos, sin problemas de permisos
```

---

**Creado:** 3 de abril de 2026  
**Versión:** 1.0  
**Estado:** Producción
