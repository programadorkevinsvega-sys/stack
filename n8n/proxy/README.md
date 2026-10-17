# 🌐 Proxy Reverso para n8n

Este directorio contiene las configuraciones para exponer n8n a internet de forma segura mediante un **proxy reverso**.

## ¿Por qué un Proxy Reverso?

El proxy reverso actúa como **una única entrada pública** a tu infraestructura:

```
INTERNET
  ↓ (80, 443)
[Proxy: Nginx/Traefik] ← UNA SOLA ENTRADA PÚBLICA
  ↓ (privada)
n8n-web (stack-network) ← NO EXPUESTO DIRECTAMENTE
  ↓ (privada)
Redis, PostgreSQL, etc. ← COMPLETAMENTE PROTEGIDO
```

**Beneficios:**
- ✅ Los servicios internos NUNCA están expuestos a internet
- ✅ Una sola "puerta" a proteger y controlar
- ✅ SSL/TLS centralizado en el proxy
- ✅ Firewall implícito: solo 80/443 públicos
- ✅ Fácil agregar más servicios (n8n workers, APIs, etc.)

---

## 📁 Archivos Disponibles

### 1. **docker-compose.nginx.yml** - Nginx (Recomendado para Principiantes)
- ✅ Ligero (nginx:alpine)
- ✅ Configuración simple
- ✅ Rápido y eficiente
- ⚠️ Menos features que Traefik

**Usar si:**
- Solo necesitas n8n expuesto
- Quieres máximo rendimiento
- Prefieres configuración manual simple

### 2. **docker-compose.traefik.yml** - Traefik (Avanzado)
- ✅ Auto-descubrimiento de servicios
- ✅ Certificados Let's Encrypt automáticos
- ✅ Dashboard web
- ✅ Manejo de múltiples puertos/subdominios
- ⚠️ Más complejo, mayor consumo de recursos

**Usar si:**
- Necesitas múltiples servicios expuestos
- Quieres HTTPS automático
- Prefieres configuración por labels

### 3. **nginx.conf** - Archivo de Configuración Nginx
- Configuración proxy reverso
- WebSocket support
- Headers necesarios para n8n
- Comentarios para HTTPS (opcional)

---

## 🚀 Quick Start

### Paso 1: Asegúrate que las redes existen
```bash
cd ..
bash CREATE_NETWORKS.sh
# O manualmente:
# docker network create stack-network
# docker network create proxy-network
```

### Paso 2: Levantar Base de Datos
```bash
cd ../database/
docker-compose -f docker-compose.postgres.yml up -d
docker-compose -f docker-compose.redis.yml up -d
# ... resto de servicios
```

### Paso 3: Levantar Proxy Reverso

#### Opción A: Nginx (Simple)
```bash
cd ../proxy/
docker-compose -f docker-compose.nginx.yml up -d

# Verificar
docker logs nginx-proxy
```

#### Opción B: Traefik (Avanzado)
```bash
cd ../proxy/
docker-compose -f docker-compose.traefik.yml up -d

# Verificar (dashboard en http://localhost:8080)
docker logs traefik
```

### Paso 4: Levantar n8n

**Para Nginx:**
```bash
cd ../local/
docker-compose up -d

# El docker-compose.yml ya está configurado para ambas redes
```

**Para Traefik:**
```bash
cd ../local/
# Primero, modificar docker-compose.yml para agregar labels a n8n-web
# (Ver sección "Configuración para Traefik" abajo)
docker-compose up -d
```

### Paso 5: Acceder

```bash
# Desde el host local
http://localhost/

# Desde otra máquina via Tailscale
http://100.121.34.76/

# Dashboard Traefik (si usas Traefik, localhost solo)
http://localhost:8080/
```

---

## 🔧 Configuración Detallada

### Nginx Configuration

El archivo `nginx.conf` define:

1. **Upstream n8n_web** - Dónde está n8n-web
   ```nginx
   upstream n8n_web {
       server n8n-web:5678;
   }
   ```

2. **Server HTTP** - Escucha puerto 80
   ```nginx
   server {
       listen 80;
       # Proxy todo a n8n-web
       location / {
           proxy_pass http://n8n_web;
           # Headers necesarios...
       }
   }
   ```

3. **WebSocket Support** - Critical para n8n
   ```nginx
   proxy_set_header Upgrade $http_upgrade;
   proxy_set_header Connection "upgrade";
   ```

4. **HTTPS** - Descomentado para SSL/TLS
   - Requiere certificados en `./ssl/`
   - Generar con: `certbot` o `Let's Encrypt`

### Traefik Configuration

Traefik auto-descubre servicios por labels en docker-compose:

```yaml
services:
  n8n-web:
    labels:
      # Habilitar
      - "traefik.enable=true"
      
      # Enrutamiento
      - "traefik.http.routers.n8n.rule=Host(`localhost`) || Host(`n8n.example.com`)"
      
      # Puerto interno
      - "traefik.http.services.n8n.loadbalancer.server.port=5678"
      
      # Entry point (web=HTTP, websecure=HTTPS)
      - "traefik.http.routers.n8n.entrypoints=web"
```

---

## 📊 Arquitectura de Conexiones

### Redes

```
┌─────────────────────────────────────────┐
│        proxy-network (PÚBLICA)          │
│  - 0.0.0.0:80 (HTTP público)           │
│  - 0.0.0.0:443 (HTTPS público)         │
│  - nginx-proxy o traefik (única entrada)│
└──────────────┬──────────────────────────┘
               │
               ↓ (interna, puente)
┌──────────────────────────────────────────┐
│   stack-network (PRIVADA)               │
│  - n8n-web:5678                         │
│  - n8n-worker (x2)                      │
│  - ...nunca expuesto directamente...    │
│  - postgres, redis, rabbitmq, etc.      │
└─────────────────────────────────────────┘
```

### DNS Resolution

**Dentro de proxy:**
```bash
# nginx/traefik pueden resolver n8n-web automáticamente
# porque están en la misma red docker (proxy-network)
curl http://n8n-web:5678/  # Funciona automáticamente
```

**Desde stack-network:**
```bash
# n8n-web puede resolver redis, postgres, etc.
# porque están en la misma red (stack-network)
redis-cli -h redis ping  # Funciona automáticamente
```

---

## 🔍 Troubleshooting

### "Connection refused" desde nginx a n8n-web
```bash
# Verificar que n8n-web está en proxy-network
docker network inspect proxy-network | grep n8n-web

# Si no está, conectarlo:
docker network connect proxy-network n8n-web
```

### Nginx returns 502 Bad Gateway
```bash
# Ver logs de nginx
docker logs nginx-proxy

# Ver logs de n8n-web
docker logs n8n-web

# Verificar que n8n-web está sano
docker exec n8n-web curl -s http://localhost:5678/api/v1/health

# Reiniciar ambos
docker restart nginx-proxy n8n-web
```

### Traefik no descubre n8n-web
```bash
# Ver dashboard
http://localhost:8080/

# Ver logs
docker logs traefik

# Verificar que n8n-web tiene labels
docker inspect n8n-web | grep -A 20 '"Labels"'

# Reiniciar traefik después de modificar labels
docker restart traefik
```

### WebSocket timeout en navegador
```bash
# Aumentar timeouts en nginx.conf:
proxy_read_timeout 86400;
proxy_connect_timeout 60;

# O en traefik, aumentar en labels:
- "traefik.http.services.n8n.loadbalancer.healthcheck.path=/health"
- "traefik.http.services.n8n.loadbalancer.healthcheck.interval=10s"
```

### SSL/TLS Certificate Issues
```bash
# Generar certificado autofirmado (desarrollo)
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout ./ssl/key.pem -out ./ssl/cert.pem

# O usar Let's Encrypt (producción, requiere dominio public)
# Descomentar sección HTTPS en nginx.conf y usar certbot
sudo certbot certonly --nginx -d n8n.example.com
```

---

## 📚 Referencia de Comandos

### Docker

```bash
# Crear redes
docker network create stack-network
docker network create proxy-network

# Levantar proxy
docker-compose -f docker-compose.nginx.yml up -d
# O traefik
docker-compose -f docker-compose.traefik.yml up -d

# Ver logs
docker logs nginx-proxy
docker logs traefik
docker logs n8n-web

# Ver redes
docker network inspect proxy-network
docker network inspect stack-network

# Conectar contenedor a red
docker network connect proxy-network n8n-web

# Detener
docker-compose down
```

### Testing

```bash
# Desde host
curl -v http://localhost/

# Desde contenedor
docker exec nginx-proxy curl http://n8n-web:5678/api/v1/health

# Health check
docker exec nginx-proxy curl http://localhost/health
```

---

## 🔐 Notas de Seguridad

1. **NO expongas directamente** los puertos de servicios internos:
   ```bash
   # ❌ MAL
   - "0.0.0.0:6379:6379"  # Redis expuesto
   - "0.0.0.0:5432:5432"  # PostgreSQL expuesto
   
   # ✅ BIEN
   - "127.0.0.1:6379:6379"     # Local solo
   - "100.121.34.76:5432:5432" # Tailscale solo
   ```

2. **Usar HTTPS** en producción:
   - Descomentar sección HTTPS en nginx.conf
   - Generar certificados con Let's Encrypt/Certbot
   - O usar Traefik que lo hace automáticamente

3. **Firewall Linux:**
   ```bash
   sudo ufw allow 80/tcp
   sudo ufw allow 443/tcp
   sudo ufw deny 5432/tcp
   sudo ufw deny 6379/tcp
   ```

---

## 📖 Referencias

- [Nginx Proxy Documentation](https://nginx.org/en/docs/)
- [Traefik Documentation](https://doc.traefik.io/traefik/)
- [Docker Networking](https://docs.docker.com/network/)
- [WebSocket Proxying with Nginx](https://nginx.org/en/docs/http/websocket.html)

---

## 💡 Tips

- **Performance**: Nginx es más ligero. Traefik es más flexible.
- **Development**: Usa nginx.conf sin HTTPS (descomentar después)
- **Production**: Usa Traefik + Let's Encrypt automático
- **Monitoring**: Ambos pueden loguear a archivo/ELK/Prometheus
- **Scaling**: El proxy reverso permite agregar más n8n-workers detrás

---

**Versión:** 1.0  
**Fecha:** 2026-03-24  
**Autor:** n8n Stack Setup
