# Certificados TLS para MySQL

## Archivos esperados

Los certificados se generarán automáticamente en el directorio `generales/`:

- `generales/root.crt` - Certificado raíz (CA)
- `generales/root.key` - Clave privada de la CA
- `generales/server.crt` - Certificado del servidor
- `generales/server.key` - Clave privada del servidor
- `generales/root.srl` - Serial number file (generado automáticamente)

Rutas dentro del contenedor:

- `/etc/mysql/certs/generales/server.crt`
- `/etc/mysql/certs/generales/server.key`
- `/etc/mysql/certs/generales/root.crt`

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
cd database/mysql/ssl
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
# Permite crear certificados específicos por microservicio
```

### 4. Renovar certificado de servidor

Si cambió la IP pública o se agregaron nuevos dominios:

```bash
./03-renew-server-cert.sh 100.121.34.76 10.0.0.4 mi-dominio.com
```

## Variables de entorno

```bash
# CA
CA_DAYS=3650                    # Validez en días (default: 10 años)
CA_CN=Vertercloud-MySQL-Root-CA

# Server
SERVER_DAYS=825                 # Validez en días (default: ~2 años)
SERVER_CN=mysql-server

# Client
CLIENT_DAYS=825
CLIENT_CN=mysql-client
```

## Notas importantes

- **La CA es la base de confianza**: Una vez creada, NO debería regenerarse.
- **El certificado del servidor se puede renovar**: Sin invalidar la CA.
- **MySQL requiere SSL habilitado**: El docker-compose ya incluye `--require-secure-transport=ON`.
- **Permisos**: Los archivos de claves privadas se generan con permisos 600.

## Resolución de problemas

**Error: "missing CA files"**
- Ejecuta primero: `./01-generate-ca.sh`

**MySQL rechaza conexión SSL**
- Verifica que los certificados estén en `generales/`
- Reinicia el contenedor: `docker-compose restart mysql`

**Certificado expirado**
- Ejecuta: `./03-renew-server-cert.sh` con los SANs actuales
