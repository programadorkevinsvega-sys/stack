# Certificados TLS para Kafka

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/keystore.p12` - Almacén de claves (generado por prepare-certs.sh)
- `generales/truststore.p12` - Almacén de confianza (generado por prepare-certs.sh)

Rutas dentro del contenedor:

- `/etc/kafka/secrets/keystore.p12`
- `/etc/kafka/secrets/truststore.p12`

## Scripts disponibles

- `01-generate-ca.sh`
  Crea la Autoridad Certificadora (CA) raíz.
  
- `02-generate-server-cert.sh`
  Genera el certificado del servidor con SANs DNS/IP configurables.
  
- `03-renew-server-cert.sh`
  Regenera el certificado del servidor si cambiaron IPs o dominios.
  
- `04-generate-client-cert.sh`
  Genera certificados de cliente para autenticación mTLS.

- `prepare-certs.sh`
  **IMPORTANTE**: Ejecutar después de generar certificados.
  Convierte los certificados PEM a formato PKCS12 (keystores y truststores).

## Uso rápido

### 1. Generar la CA raíz

```bash
cd database/kafka/ssl
./01-generate-ca.sh
```

### 2. Generar certificado del servidor

```bash
./02-generate-server-cert.sh
# El script pregunta por:
# - IP de Tailscale
# - IP privada VPC (opcional)
# - IPs/dominios adicionales (opcional)
```

O pasar IPs directamente:

```bash
./02-generate-server-cert.sh 100.121.34.76 10.0.0.4
```

### 3. Preparar certificados para Kafka (OBLIGATORIO)

**Este paso es necesario para usar SSL con Kafka**:

```bash
./prepare-certs.sh
# Crea: generales/keystore.p12 y generales/truststore.p12
```

### 4. Generar certificados de cliente (opcional)

```bash
./04-generate-client-cert.sh
```

### 5. Renovar certificado de servidor

```bash
./03-renew-server-cert.sh 100.121.34.76 10.0.0.4
# Luego actualizar keystores:
./prepare-certs.sh
```

## Variables de entorno

```bash
# CA
CA_DAYS=3650
CA_CN=Vertercloud-Kafka-Root-CA

# Server
SERVER_DAYS=825
SERVER_CN=kafka-cluster

# Keystores
KAFKA_KEYSTORE_PASSWORD=Kevins1612522026
KAFKA_TRUSTSTORE_PASSWORD=Kevins1612522026
```

## Puertos disponibles

- **9092**: PLAINTEXT (sin cifrado) - Compatibilidad
- **9094**: SSL/TLS (cifrado) - Seguro
- **9093**: CONTROLLER (interno, para cluster)

## Notas importantes

- **prepare-certs.sh es obligatorio**: Kafka requiere formatos PKCS12.
- **Requiere herramientas**: `openssl` y `keytool` (incluido en Java).
- **Keystores y Truststores**: Se generan con las contraseñas configuradas.
- **La CA es la base de confianza**: Una vez creada, NO debería regenerarse.
- **Permisos**: Los archivos se generan con permisos 600.

## Conexión desde clientes

### Con SSL/TLS (puerto 9094):
```python
from kafka import KafkaProducer
import ssl

# Crear contexto SSL
ssl_context = ssl.create_default_context(ssl.Purpose.SERVER_AUTH)
ssl_context.check_hostname = False
ssl_context.verify_mode = ssl.CERT_REQUIRED
ssl_context.load_verify_locations('/ruta/a/root.crt')

producer = KafkaProducer(
    bootstrap_servers=['kafka:9094'],
    security_protocol='SSL',
    ssl_context=ssl_context
)
```

### Sin TLS (puerto 9092):
```python
from kafka import KafkaProducer

producer = KafkaProducer(bootstrap_servers=['kafka:9092'])
```

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**Error: "missing keystore.p12"**
- Ejecuta: `./prepare-certs.sh`

**Kafka rechaza conexión SSL**
- Verifica que keystores existan: `ls -la generales/*.p12`
- Verifícalas variables de entorno en docker-compose
- Reinicia el contenedor: `docker-compose restart kafka`

**keytool no encontrado**
- Instala Java: `apt-get install openjdk-11-jre-headless`

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
- Luego: `./prepare-certs.sh`
