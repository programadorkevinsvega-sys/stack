# SSL/TLS Opcional en Database Stack

Este documento describe cómo habilitar o deshabilitar SSL/TLS en los servicios de base de datos.

## Variables de Entorno

Cada servicio ahora soporta una variable de entorno para controlar SSL/TLS:

### MongoDB
```bash
MONGODB_ENABLE_SSL=true|false
```
- **Default**: `false` (SSL deshabilitado)
- **true**: Requiere conexiones con SSL/TLS 1.3
- **false**: Acepta conexiones sin SSL

### MySQL
```bash
MYSQL_ENABLE_SSL=true|false
```
- **Default**: `false` (SSL deshabilitado)
- **true**: Requiere SSL/TLS 1.3
- **false**: Acepta conexiones sin SSL

### PostgreSQL
```bash
POSTGRES_ENABLE_SSL=true|false
```
- **Default**: `false` (SSL deshabilitado)
- **true**: Solo conexiones SSL/TLS (hostssl)
- **false**: Acepta conexiones con o sin SSL (host)

### Redis
```bash
REDIS_ENABLE_SSL=true|false
```
- **Default**: `true` (SSL habilitado)
- **true**: Puerto TLS 6379, puerto HTTP deshabilitado
- **false**: Puerto TCP plain 6379, TLS deshabilitado

### RabbitMQ
```bash
RABBITMQ_ENABLE_SSL=true|false
```
- **Default**: `true` (SSL habilitado)
- **true**: Conexiones AMQP SSL en puerto 5671, AMQP plain en localhost 5672
- **false**: Solo conexiones AMQP plain en puerto 5672

### Kafka
```bash
KAFKA_ENABLE_SSL=true|false
```
- **Default**: `true` (SSL habilitado)
- **Nota**: Kafka siempre tiene ambos puertos PLAINTEXT:9092 y SSL:9094 disponibles

### MinIO
```bash
MINIO_ENABLE_SSL=true|false
```
- **Default**: `true` (SSL habilitado)
- **true**: MinIO busca certificados en /etc/minio/certs/
- **false**: MinIO funciona sin SSL/TLS

### Vault
```bash
VAULT_ENABLE_SSL=true|false
```
- **Default**: `true` (SSL habilitado)
- **true**: Requiere TLS 1.3
- **false**: Plain HTTP (solo para desarrollo)

## Ejemplos de Uso

### Deshabilitar SSL en TODO (desarrollo local)
```bash
export MONGODB_ENABLE_SSL=false
export MYSQL_ENABLE_SSL=false
export POSTGRES_ENABLE_SSL=false
export REDIS_ENABLE_SSL=false
export RABBITMQ_ENABLE_SSL=false
export MINIO_ENABLE_SSL=false
export VAULT_ENABLE_SSL=false

docker-compose -f database/mongodb/docker-compose.mongodb.yml up -d
docker-compose -f database/mysql/docker-compose.mysql.yml up -d
docker-compose -f database/postgres/docker-compose.postgres.yml up -d
docker-compose -f database/redis/docker-compose.redis.yml up -d
docker-compose -f database/rabbitmq/docker-compose.rabbitmq.yml up -d
docker-compose -f database/minio/docker-compose.minio.yml up -d
docker-compose -f database/vault/docker-compose.vault.yml up -d
```

### Habilitar SSL en TODO (producción)
```bash
export MONGODB_ENABLE_SSL=true
export MYSQL_ENABLE_SSL=true
export POSTGRES_ENABLE_SSL=true
export REDIS_ENABLE_SSL=true
export RABBITMQ_ENABLE_SSL=true
export MINIO_ENABLE_SSL=true
export VAULT_ENABLE_SSL=true

docker-compose -f database/mongodb/docker-compose.mongodb.yml up -d
docker-compose -f database/mysql/docker-compose.mysql.yml up -d
docker-compose -f database/postgres/docker-compose.postgres.yml up -d
docker-compose -f database/redis/docker-compose.redis.yml up -d
docker-compose -f database/rabbitmq/docker-compose.rabbitmq.yml up -d
docker-compose -f database/minio/docker-compose.minio.yml up -d
docker-compose -f database/vault/docker-compose.vault.yml up -d
```

### Crear archivo .env para persistencia
```bash
cat > database/.env << 'EOF'
# SSL/TLS Configuration for Database Stack
MONGODB_ENABLE_SSL=false
MYSQL_ENABLE_SSL=false
POSTGRES_ENABLE_SSL=false
REDIS_ENABLE_SSL=false
RABBITMQ_ENABLE_SSL=false
MINIO_ENABLE_SSL=false
VAULT_ENABLE_SSL=false

# Otras variables...
TZ=America/Bogota
MONGODB_INITDB_ROOT_USERNAME=KevinsAdmin
MONGODB_INITDB_ROOT_PASSWORD=Kevins1612522026
EOF
```

## Cambios Realizados

### 1. MongoDB (`mongodb/docker-compose.mongodb.yml`)
- Reemplazado comando estático con script dinámico
- Lee `MONGODB_ENABLE_SSL` para habilitar/deshabilitar TLS 1.3

### 2. MySQL (`mysql/docker-compose.mysql.yml`)
- Reemplazado comando estático con script dinámico
- Lee `MYSQL_ENABLE_SSL` para habilitar/deshabilitar TLS 1.3

### 3. PostgreSQL (`postgres/docker-compose.postgres.yml`)
- Creado script `generate-pg-hba.sh` que genera pg_hba.conf dinámicamente
- Lee `POSTGRES_ENABLE_SSL` para usar hostssl o host

### 4. Redis (`redis/docker-compose.redis.yml`)
- Reemplazado comando estático con script dinámico
- Lee `REDIS_ENABLE_SSL` para activar puerto TLS o HTTP
- Healthcheck adaptado para ambos modos

### 5. RabbitMQ (`rabbitmq/docker-compose.rabbitmq.yml`)
- Creado script `generate-rabbitmq.conf.sh` que genera rabbitmq.conf dinámicamente
- Lee `RABBITMQ_ENABLE_SSL` para configurar listeners TLS u HTTP

### 6. Kafka (`kafka/docker-compose.kafka.yml`)
- Agregada variable `KAFKA_ENABLE_SSL` (opcional)
- Mantiene ambos puertos disponibles

### 7. MinIO (`minio/docker-compose.minio.yml`)
- Reemplazado comando estático con script dinámico
- Lee `MINIO_ENABLE_SSL` para usar certificados o no

### 8. Vault (`vault/docker-compose.vault.yml`)
- Creado script `generate-vault-config.sh` que genera vault.hcl dinámicamente
- Lee `VAULT_ENABLE_SSL` para habilitar/deshabilitar TLS 1.3

## Notas Importantes

1. **Certificados**: Los certificados SSL/TLS deben estar presentes en `ssl/generales/` para que funcione SSL. Si no están presentes y `*_ENABLE_SSL=true`, habrá errores.

2. **Conexiones de Cliente**: Las aplicaciones cliente deben ser configuradas para usar los puertos correctos:
   - **Sin SSL**: Puertos normales (27017 para MongoDB, 3306 para MySQL, etc.)
   - **Con SSL**: Puertos TLS (requiere certificados CA para validación)

3. **Variables por Defecto**: 
   - Algunos servicios tienen SSL **habilitado por defecto** (Redis, RabbitMQ, MinIO, Vault)
   - Otros tienen SSL **deshabilitado por defecto** (MongoDB, MySQL, PostgreSQL)
   - Esto se puede cambiar editando los valores en las definiciones de servicios

4. **Desarrollo Local**: Para desarrollo, se recomienda desabilitar SSL en todos los servicios para simplificar la configuración.

5. **Producción**: Para producción, se recomienda habilitar SSL en todos los servicios y usar certificados válidos.
