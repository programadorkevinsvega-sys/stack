# Certificados TLS para MongoDB

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/server.pem` - Archivo PEM combinado (generado por prepare-certs.sh)

Rutas dentro del contenedor:

- `/etc/ssl/mongodb/server.pem` (clave + certificado)
- `/etc/ssl/mongodb/root.crt`

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
  Prepara los certificados en el formato requerido por MongoDB (archivo PEM combinado).

## Uso rápido

### 1. Generar la CA raíz

```bash
cd database/mongodb/ssl
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

### 3. Preparar certificados para MongoDB

**Este paso es obligatorio**:

```bash
./prepare-certs.sh
# Crea: generales/server.pem (combinando server.key + server.crt)
```

### 4. Generar certificados de cliente (opcional)

```bash
./04-generate-client-cert.sh
```

### 5. Renovar certificado de servidor

```bash
./03-renew-server-cert.sh 100.121.34.76 10.0.0.4
# Luego ejecutar de nuevo:
./prepare-certs.sh
```

## Variables de entorno

```bash
# CA
CA_DAYS=3650
CA_CN=Vertercloud-MongoDB-Root-CA

# Server
SERVER_DAYS=825
SERVER_CN=mongodb-server

# Client
CLIENT_DAYS=825
CLIENT_CN=mongodb-client
```

## Notas importantes

- **MongoDB requiere archivo PEM**: El archivo `server.pem` combina la clave privada y el certificado.
- **Ejecutar prepare-certs.sh es obligatorio**: Se ejecuta automáticamente después de generar certificados.
- **La conexión SSL es obligatoria**: MongoDB se configura con `sslMode requireSSL`.
- **Permisos**: Los archivos de claves privadas se generan con permisos 600.

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**MongoDB rechaza conexión SSL**
- Verifica que `server.pem` exista: `ls -la generales/server.pem`
- Si no existe, ejecuta: `./prepare-certs.sh`
- Reinicia el contenedor: `docker-compose restart mongodb`

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
- Luego: `./prepare-certs.sh`
