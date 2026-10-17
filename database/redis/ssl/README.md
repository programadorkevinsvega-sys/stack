# Certificados TLS para Redis

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/root.srl` - Serial number file (generado automáticamente)

Rutas dentro del contenedor:

- `/ssl/server.crt`
- `/ssl/server.key`
- `/ssl/root.crt`

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
cd database/redis/ssl
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
CA_CN=Vertercloud-Redis-Root-CA

# Server
SERVER_DAYS=825
SERVER_CN=redis-server

# Client
CLIENT_DAYS=825
CLIENT_CN=redis-client

# Redis
REDIS_PASSWORD=Kevins1612522026
REDIS_TLS=1
```

## Notas importantes

- **La CA es la base de confianza**: Una vez creada, NO debería regenerarse.
- **El certificado del servidor se puede renovar**: Sin invalidar la CA.
- **Redis requiere TLS habilitado**: El docker-compose ya incluye configuración TLS.
- **Port 6379**: TLS habilitado por defecto.
- **Permisos**: Los archivos de claves privadas se generan con permisos 600.

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**Redis rechaza conexión TLS**
- Verifica que los certificados estén en `generales/`
- Verifica los permisos: `ls -la generales/`
- Reinicia el contenedor: `docker-compose restart redis`

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
