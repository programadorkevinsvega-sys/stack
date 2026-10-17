# Certificados TLS para NATS JetStream

## Archivos esperados

- `root.crt`
- `root.key`
- `root.srl`
- `server.crt`
- `server.key`
- `client.crt` opcional
- `client.key` opcional

Rutas por defecto dentro del contenedor:

- `/etc/nats/ssl/server.crt`
- `/etc/nats/ssl/server.key`
- `/etc/nats/ssl/root.crt`

## Scripts

- `01-generate-ca.sh`
  Crea la CA raíz local para NATS.
- `02-generate-server-cert.sh`
  Genera el certificado servidor con SANs DNS/IP.
- `03-renew-server-cert.sh`
  Regenera el certificado servidor si cambió la IP o los SANs.
- `04-generate-client-cert.sh`
  Genera un certificado cliente opcional para futuro mTLS.

## Uso rápido

```bash
cd stack/database/nats/ssl
./01-generate-ca.sh
./02-generate-server-cert.sh localhost 127.0.0.1 100.121.34.76
```

Si cambia la IP pública o privada:

```bash
cd stack/database/nats/ssl
./03-renew-server-cert.sh localhost 127.0.0.1 100.121.34.76 10.0.0.4
```

## Nota

La renovación del certificado servidor no cambia la CA. Eso evita tener que redistribuir `root.crt` a todos los clientes cada vez que cambien los SANs del servidor.
