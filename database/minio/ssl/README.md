# Certificados TLS para MinIO

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/public.crt` - Copia del certificado servidor (generado por prepare-certs.sh)
- `generales/private.key` - Copia de la clave privada (generado por prepare-certs.sh)

Rutas dentro del contenedor:

- `/etc/minio/certs/public.crt`
- `/etc/minio/certs/private.key`

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
  Prepara los certificados con los nombres específicos que MinIO espera.

## Uso rápido

### 1. Generar la CA raíz

```bash
cd database/minio/ssl
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

### 3. Preparar certificados para MinIO (OBLIGATORIO)

**Este paso es necesario**:

```bash
./prepare-certs.sh
# Crea: generales/public.crt y generales/private.key
```

### 4. Generar certificados de cliente (opcional)

```bash
./04-generate-client-cert.sh
```

### 5. Renovar certificado de servidor

```bash
./03-renew-server-cert.sh 100.121.34.76 10.0.0.4
# Luego actualizar names:
./prepare-certs.sh
```

## Variables de entorno

```bash
# CA
CA_DAYS=3650
CA_CN=Vertercloud-MinIO-Root-CA

# Server
SERVER_DAYS=825
SERVER_CN=minio-server

# MinIO
MINIO_ROOT_USER=KevinsAdmin
MINIO_ROOT_PASSWORD=Kevins1612522026
```

## Puertos disponibles

- **9000**: MinIO API S3 (HTTP)
- **9001**: MinIO Console (Web UI HTTP)
- **9443**: MinIO API S3 (HTTPS)
- **9444**: MinIO Console (Web UI HTTPS)

## Notas importantes

- **prepare-certs.sh es obligatorio**: MinIO espera nombres específicos (public.crt, private.key).
- **La CA es la base de confianza**: Una vez creada, NO debería regenerarse.
- **HTTPS está habilitado automáticamente**: Si existen públic.crt y private.key en `/etc/minio/certs/`.
- **Ambos protocolos funcionan**: MinIO acepta conexiones HTTP y HTTPS simultáneamente.
- **Permisos**: Los archivos se generan con permisos apropiados (600 para claves).

## Conexión desde clientes

### Con HTTPS (puerto 9443):
```python
from minio import Minio
import ssl

client = Minio(
    'minio:9443',
    access_key='KevinsAdmin',
    secret_key='Kevins1612522026',
    secure=True,
    http_client=httpx.Client(
        verify=ssl.create_default_context(cafile='/ruta/a/root.crt')
    )
)
```

### With HTTP (puerto 9000):
```python
from minio import Minio

client = Minio(
    'minio:9000',
    access_key='KevinsAdmin',
    secret_key='Kevins1612522026',
    secure=False
)
```

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**Error: "missing public.crt or private.key"**
- Ejecuta: `./prepare-certs.sh`

**MinIO rechaza conexión HTTPS**
- Verifica archivos: `ls -la generales/public.crt generales/private.key`
- Revisa permisos: `chmod 644 generales/public.crt && chmod 600 generales/private.key`
- Reinicia el contenedor: `docker-compose restart minio`

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
- Luego: `./prepare-certs.sh`

**Console muestra advertencia de seguridad**
- Es normal si el certificado es autofirmado
- Verificar que el CN coincida con la URL de acceso
