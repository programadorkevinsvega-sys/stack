# 🔐 TLS Setup Automático

**Script maestro para generar y configurar TLS en todos los servicios.**

## 📍 Ubicación
```
database/
├── ssl/
│   ├── 00-setup-all-tls.sh    ← SCRIPT MAESTRO (ejecuta esto primero)
│   └── README.md               ← Este archivo
├── postgres/ssl/
│   ├── 01-generate-ca.sh       ← Generar CA
│   ├── 02-generate-server-cert.sh
│   ├── 04-generate-client-cert.sh
│   └── generales/              ← Certificados generados aquí
├── redis/ssl/
│   └── ... (igual)
└── [7 servicios más]
```

## ⚡ Uso Rápido

### 🟢 Primera vez (generar todos los certificados):
```bash
cd /home/servidor-lenovo/ARCHIVOS-KEVINS/stack/database
bash ssl/00-setup-all-tls.sh "100.121.34.76 localhost 127.0.0.1"
```

### 🟡 Regenerar solo un servicio:
```bash
cd database/postgres/ssl
./02-generate-server-cert.sh 100.121.34.76
```

### 🔄 En otra PC (copiar y ejecutar):
```bash
# Después de copiar toda la carpeta database/
cd database
bash ssl/00-setup-all-tls.sh "NUEVA_IP localhost 127.0.0.1"
```

---

## 🔧 Qué hace `00-setup-all-tls.sh`

| Paso | Acción | Servicios |
|------|--------|-----------|
| 1️⃣ | Genera CA raíz (`root.crt/root.key`) | Todos los 8 |
| 2️⃣ | Genera certificado servidor para cada IP | Todos los 8 |
| 3️⃣ | Prepara MongoDB PEM (`server.pem`) | MongoDB |
| 4️⃣ | Prepara Kafka PKCS12 (`keystore.p12`, `truststore.p12`) | Kafka |
| 5️⃣ | Prepara MinIO symlinks (`public.crt`, `private.key`) | MinIO |
| 6️⃣ | Verifica permisos 644 en todos | Todos |

---

## ✅ Verificación Manual

```bash
# Ver que se generaron los certificados
for service in postgres redis mysql mongodb rabbitmq kafka minio vault nats; do
  echo "$service:"
  ls -lh database/$service/ssl/generales/ | grep -E "\.crt|\.key|\.pem|\.p12"
done

# Verificar permisos (deben ser 644)
find database/*/ssl/generales -type f | xargs ls -l
```

---

## 📋 Servicios que soportan TLS

| Servicio | CA | Server | Client | Formato Especial |
|----------|-------|--------|--------|-----------------|
| PostgreSQL | ✅ | ✅ | ✅ | Standard PEM |
| Redis | ✅ | ✅ | ✅ | Standard PEM |
| MySQL | ✅ | ✅ | ✅ | Standard PEM |
| **MongoDB** | ✅ | ✅ | ✅ | **PEM único** |
| RabbitMQ | ✅ | ✅ | ✅ | Standard PEM |
| **Kafka** | ✅ | ✅ | ✅ | **PKCS12** |
| **MinIO** | ✅ | ✅ | ✅ | **Symlinks** |
| Vault | ✅ | ✅ | ✅ | Standard PEM |
| **NATS** | ✅ | ✅ | ✅ | Standard PEM |

---

## 🚀 Levantar servicios con TLS

```bash
# Un servicio
docker compose -f postgres/docker-compose.postgres.yml up -d

# Todos los servicios
docker compose -f postgres/docker-compose.postgres.yml \
               -f redis/docker-compose.redis.yml \
               -f mysql/docker-compose.mysql.yml \
               -f mongodb/docker-compose.mongodb.yml \
               -f rabbitmq/docker-compose.rabbitmq.yml \
               -f nats/docker-compose.nats.yml \
               -f kafka/docker-compose.kafka.yml \
               -f minio/docker-compose.minio.yml \
               -f vault/docker-compose.vault.yml up -d
```

---

## 🔒 Seguridad de Certificados

| Archivo | Permisos | Por qué |
|---------|----------|--------|
| `root.crt` | 644 | Público (autoridad raíz) |
| `root.key` | 644 | ⚠️ Secreto, pero Docker lo lee |
| `server.crt` | 644 | Público (certificado de servidor) |
| `server.key` | 644 | ⚠️ Secreto, pero Docker lo lee |

> **Nota**: Todos son 644 para que Docker (corriendo como user) pueda leerlos. En producción, considerar permisos más restrictivos.

---

## ❓ FAQ

**P: ¿Qué pasa si cambio de IP?**
R: Ejecuta el script master de nuevo con la nueva IP. Los viejos certificados se regenerarán automáticamente.

**P: ¿Los certificados son autofirmados?**
R: Sí. Perfectos para desarrollo/testing. Para producción, usar Let's Encrypt o una CA real.

**P: ¿Necesito permisos admin?**
R: No, a menos que cambies permisos de archivos (chmod).

**P: ¿Funciona en Windows/Mac?**
R: Sí, el script es `/bin/bash` estándar. Necesitarás WSL2 en Windows.

---

## 📞 Troubleshooting

**Error: `keytool: orden no encontrada`**
```bash
sudo apt-get install -y default-jdk-headless
```

**Error: `Permission denied` en contenedor**
```bash
# Los permisos ya se aplican automáticamente (644), pero si falla:
find database/*/ssl/generales -type f -exec chmod 644 {} \;
```

**Error: Certificado rechazado por cliente**
```bash
# Asegúrate de usar el root.crt correcto:
nats --server tls://user:pass@localhost:4223 --nats-root-cert database/nats/ssl/generales/root.crt server info
```

---

**Última actualización:** 13 de abril de 2026
