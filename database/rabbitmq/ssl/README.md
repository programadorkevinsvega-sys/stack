# Certificados TLS para RabbitMQ

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/root.srl` - Serial number file (generado automáticamente)

Rutas dentro del contenedor:

- `/etc/rabbitmq/ssl/server.crt`
- `/etc/rabbitmq/ssl/server.key`
- `/etc/rabbitmq/ssl/root.crt`

## Scripts disponibles

- `01-generate-ca.sh`
  Crea la Autoridad Certificadora (CA) raíz.
  
- `02-generate-server-cert.sh`
  Genera el certificado del servidor con SANs DNS/IP configurables.
  
- `03-renew-server-cert.sh`
  Regenera el certificado del servidor si cambiaron IPs o dominios.
  
- `04-generate-client-cert.sh`
  Genera certificados de cliente para autenticación mTLS.

## Uso rápido

### 1. Generar la CA raíz

```bash
cd database/rabbitmq/ssl
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

### 3. Generar certificados de cliente (opcional)

```bash
./04-generate-client-cert.sh
```

### 4. Renovar certificado de servidor

```bash
./03-renew-server-cert.sh 100.121.34.76 10.0.0.4
```

## Variables de entorno

```bash
# CA
CA_DAYS=3650
CA_CN=Vertercloud-RabbitMQ-Root-CA

# Server
SERVER_DAYS=825
SERVER_CN=rabbitmq-server

# Client
CLIENT_DAYS=825
CLIENT_CN=rabbitmq-client

# RabbitMQ
RABBITMQ_DEFAULT_USER=KevinsAdmin
RABBITMQ_DEFAULT_PASS=Kevins1612522026
```

## Puertos disponibles

- **5672**: AMQP sin cifrado (PLAINTEXT)
- **5671**: AMQP con TLS (SSL)
- **15672**: Management UI (HTTP)
- **15671**: Management UI (HTTPS)

## Notas importantes

- **La CA es la base de confianza**: Una vez creada, NO debería regenerarse.
- **El certificado del servidor se puede renovar**: Sin invalidar la CA.
- **RabbitMQ soporta ambos puertos**: Clientes pueden usar 5672 o 5671.
- **Los certificados se cargan automáticamente**: Desde las variables de entorno en docker-compose.
- **Permisos**: Los archivos de claves privadas se generan con permisos 600.

## Conexión desde clientes

### Con TLS (recomendado - puerto 5671):
```python
import pika

credentials = pika.PlainCredentials('KevinsAdmin', 'Kevins1612522026')
context = pika.ssl.create_default_context(pika.ssl.Purpose.SERVER_AUTH, cafile='/ruta/a/root.crt')
context.check_hostname = False
context.verify_mode = ssl.CERT_REQUIRED

conn = pika.BlockingConnection(
    pika.ConnectionParameters(
        host='rabbitmq',
        port=5671,
        credentials=credentials,
        ssl_options=pika.ssl.SSLOptions(context)
    )
)
```

### Sin TLS (puerto 5672):
```python
credentials = pika.PlainCredentials('KevinsAdmin', 'Kevins1612522026')
conn = pika.BlockingConnection(
    pika.ConnectionParameters('rabbitmq', 5672, credentials=credentials)
)
```

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**RabbitMQ rechaza conexión SSL**
- Verifica que los certificados estén en `generales/`
- Reinicia el contenedor: `docker-compose restart rabbitmq`

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
