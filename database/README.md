# 🗄️ Servicios de Base de Datos para n8n

**Stack completo de servicios externos que n8n necesita, completamente portable.**

---

## 📋 Servicios Disponibles

| Servicio | Puerto | Archivo |
|----------|--------|---------|
| **PostgreSQL** | 5432 | `postgres/docker-compose.postgres.yml` |
| **Redis** | 6379 | `redis/docker-compose.redis.yml` |
| **MySQL** | 3306 | `mysql/docker-compose.mysql.yml` |
| **MongoDB** | 27017 | `mongodb/docker-compose.mongodb.yml` |
| **RabbitMQ** | 5672 | `rabbitmq/docker-compose.rabbitmq.yml` |
| **NATS JetStream** | 4223 | `nats/docker-compose.nats.yml` |
| **Kafka** | 9092 | `kafka/docker-compose.kafka.yml` |
| **MinIO** | 9000 | `minio/docker-compose.minio.yml` |
| **Vault** | 8500 | `vault/docker-compose.vault.yml` |

---

## 🚀 Inicio Rápido

### Levantar un servicio
```bash
cp nats/.env.example nats/.env  # solo la primera vez si vas a levantar NATS

# PostgreSQL
sudo docker compose -f postgres/docker-compose.postgres.yml up -d

# Redis
sudo docker compose -f redis/docker-compose.redis.yml up -d

# qdrant
sudo docker compose -f qdrant/docker-compose.qdrant.yml up -d

# NATS
sudo docker compose -f nats/docker-compose.nats.yml up -d

# Kafka
sudo docker compose -f kafka/docker-compose.kafka.yml up -d

# Vault
sudo docker compose -f vault/docker-compose.vault.yml up -d

# MinIO
sudo docker compose -f minio/docker-compose.minio.yml up -d

# RabbitMQ
sudo docker compose -f rabbitmq/docker-compose.rabbitmq.yml up -d

# Kafka
sudo docker compose -f kafka/docker-compose.kafka.yml up -d

# Kafka-ui
sudo docker compose -f kafka/docker-compose.kafka-ui.yml up -d

# MySQL
sudo docker compose -f mysql/docker-compose.mysql.yml up -d

# MongoDB
sudo docker compose -f mongodb/docker-compose.mongodb.yml up -d

```

### Levantar todos los servicios
```bash
docker compose -f database/postgres/docker-compose.postgres.yml \
               -f database/redis/docker-compose.redis.yml \
               -f database/rabbitmq/docker-compose.rabbitmq.yml \
               -f database/kafka/docker-compose.kafka.yml \
               -f database/kafka/docker-compose.kafka-ui.yml \
               -f database/minio/docker-compose.minio.yml \
               -f database/qdrant/docker-compose.qdrant.yml \
               -f database/vault/docker-compose.vault.yml \
               -f database/nats/docker-compose.nats.yml up -d
```

### Detener servicios
```bash
sudo docker compose -f */docker-compose*.yml down -v
```



### Detener todos los servicios y elimiar datos
```bash
docker compose -f database/postgres/docker-compose.postgres.yml \
               -f database/redis/docker-compose.redis.yml \
               -f database/mysql/docker-compose.mysql.yml \
               -f database/mongodb/docker-compose.mongodb.yml \
               -f database/rabbitmq/docker-compose.rabbitmq.yml \
               -f database/kafka/docker-compose.kafka.yml \
               -f database/kafka/docker-compose.kafka-ui.yml \
               -f database/minio/docker-compose.minio.yml \
               -f database/vault/docker-compose.vault.yml \
               -f database/qdrant/docker-compose.qdrant.yml \
               -f database/nats/docker-compose.nats.yml down -v
---

### detener un servicio
```bash

# PostgreSQL
sudo docker compose -f postgres/docker-compose.postgres.yml down

# Redis
sudo docker compose -f redis/docker-compose.redis.yml down

# qdrant
sudo docker compose -f qdrant/docker-compose.qdrant.yml down

# NATS
sudo docker compose -f nats/docker-compose.nats.yml down

# RabbitMQ
sudo docker compose -f rabbitmq/docker-compose.rabbitmq.yml down

# Kafka
sudo docker compose -f kafka/docker-compose.kafka.yml down

# Vault
sudo docker compose -f vault/docker-compose.vault.yml down

# MinIO
sudo docker compose -f minio/docker-compose.minio.yml down 

# Kafka-ui
sudo docker compose -f kafka/docker-compose.kafka-ui.yml down

# MySQL
sudo docker compose -f mysql/docker-compose.mysql.yml down

# MongoDB
sudo docker compose -f mongodb/docker-compose.mongodb.yml down

```

## 📦 Portabilidad Completa

**¿Necesitas llevar TODOS tus datos a otro PC?**

Tenemos herramientas automáticas para hacerlo sin perder nada.

### Guías disponibles:
- **[postgres/doc/README-PORTABILIDAD.md](postgres/doc/README-PORTABILIDAD.md)** - Referencia rápida (5 minutos)
- **[postgres/doc/PORTABILIDAD_COMPLETA.md](postgres/doc/PORTABILIDAD_COMPLETA.md)** - Documentación detallada

### Resumen ultra-rápido
```bash
# En tu PC actual
./postgres/script/export-all-data.sh

# En el nuevo PC
tar xzf n8n-database-backup-*.tar.gz
cd database/
./postgres/script/fix-all-permissions.sh
sudo docker compose -f docker-compose.*.yml up -d
```

---

## 🔧 Herramientas

### `postgres/script/fix-all-permissions.sh`
Arregla automáticamente los permisos en un nuevo PC.
```bash
./postgres/script/fix-all-permissions.sh
```

### `postgres/script/export-all-data.sh`
Empaca TODOS tus datos para llevar a otro PC.
```bash
./postgres/script/export-all-data.sh
```

---

## ✨ Características

- ✅ Todos los datos en **volúmenes locales** (no en Docker)
- ✅ **SSL/TLS habilitado** en PostgreSQL y Redis
- ✅ **Completamente portable** - Copia y funciona
- ✅ **Sin problemas de permisos** - Script automático
- ✅ **Listo para producción** - Sin cambios extra

---

## 📁 Estructura

```
database/
├── README.md                                ← Este archivo
├── docker-compose.*.yml                     ← Configuración de servicios
│
├── postgres/
│   ├── docker-compose.postgres.yml
│   ├── doc/
│   │   ├── PORTABILIDAD_COMPLETA.md        ← Documentación detallada
│   │   └── README-PORTABILIDAD.md          ← Guía rápida
│   ├── script/
│   │   ├── fix-all-permissions.sh          ← Arregla permisos
│   │   └── export-all-data.sh              ← Empaca datos
│   ├── ssl/
│   │   ├── server.crt / server.key         ← Certificados SSL
│   │   └── root.srl
│   └── volumes/                            ← Datos PostgreSQL
│
├── redis/
│   ├── docker-compose.redis.yml
│   ├── script/
│   ├── ssl/                                ← Certificados TLS
│   └── volumes/                            ← Datos Redis
│
├── mysql/
│   ├── docker-compose.mysql.yml
│   ├── script/
│   └── volumes/                            ← Datos MySQL
│
├── mongodb/
│   ├── docker-compose.mongodb.yml
│   └── volumes/                            ← Datos MongoDB
│
├── rabbitmq/
│   ├── docker-compose.rabbitmq.yml
│   ├── script/
│   └── volumes/                            ← Datos RabbitMQ
│
├── nats/
│   ├── docker-compose.nats.yml
│   ├── doc/
│   ├── script/
│   └── volumes/                            ← Datos JetStream
│
├── kafka/
│   ├── docker-compose.kafka.yml
│   ├── script/
│   └── volumes/                            ← Datos Kafka
│
├── minio/
│   ├── docker-compose.minio.yml
│   ├── script/
│   └── volumes/                            ← Buckets MinIO
│
└── vault/
    ├── docker-compose.vault.yml
    ├── script/
    │   └── entrypoint.sh
    ├── ssl/
    └── volumes/                            ← Datos Vault
```

---

## ⚡ Notas Importantes

- Cada servicio tiene su **propio docker-compose.yml** dentro de su carpeta
- `nats/` usa `username/password` obligatorio y `TLS` opcional por variable
- Puedes levantar solo los que necesites
- Los datos persisten en `./[servicio]/volumes/`
- Todos usan la **red `stack-network`**
- **Certificados SSL/TLS** están en `./[servicio]/ssl/`
- **Scripts de portabilidad** en `postgres/script/`
- **Documentación completa** en `postgres/doc/`

---

## 📚 Documentación Específica

- **Portabilidad**: Ver `postgres/doc/PORTABILIDAD_COMPLETA.md`
- **PostgreSQL**: Ver `postgres/docker-compose.postgres.yml`
- **Redis**: Ver `redis/docker-compose.redis.yml`
- **MySQL**: Ver `mysql/docker-compose.mysql.yml`
- **MongoDB**: Ver `mongodb/docker-compose.mongodb.yml`
- **RabbitMQ**: Ver `rabbitmq/docker-compose.rabbitmq.yml`
- **NATS JetStream**: Ver `nats/docker-compose.nats.yml`
- **Kafka**: Ver `kafka/docker-compose.kafka.yml`
- **MinIO**: Ver `minio/docker-compose.minio.yml`
- **Vault**: Ver `vault/docker-compose.vault.yml`

---

**Última actualización:** 3 de abril de 2026
