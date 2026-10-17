# 🐳 Gestión Docker - Portainer

**Gestor visual de Docker con interfaz web. Administra contenedores, volúmenes, redes y más sin usar terminal.**

---

## � Tabla de Contenidos

1. [Inicio Rápido](#inicio-rápido)
2. [Archivos](#archivos)
3. [Acceso](#acceso)
4. [Configuración](#configuración)
5. [Comandos Útiles](#comandos-útiles)
6. [Problemas Comunes](#problemas-comunes)
7. [Diagnóstico](#diagnóstico)

---

## 🚀 Inicio Rápido

### 1. Levantar Portainer

```bash
cd gestion_docker/
docker compose up -d
```

### 2. Acceder a la Interfaz

```
http://localhost:19000
```

o con HTTPS (seguro):

```
https://localhost:19443
```

### 3. Ver Logs

```bash
docker logs -f portainer
```

### 4. Detener Portainer

```bash
docker compose down
```

---

## 📋 Archivos

| Archivo | Descripción |
|---------|-------------|
| `docker-compose.yml` | Configuración de Portainer |
| `README.md` | Este archivo (toda la documentación) |
| `.env.example` | Variables de entorno |

---

## 🌐 Acceso

### Puertos

| Puerto | Protocolo | Uso |
|--------|-----------|-----|
| 19000 | HTTP | Interfaz web (estándar) |
| 19443 | HTTPS | Interfaz web (seguro) |
| 8000 | TCP | Edge Agent (opcional) |

### URLs Rápidas

- **Local HTTP**: `http://localhost:19000`
- **Local HTTPS**: `https://localhost:19443`
- **Por IP local**: `http://100.121.34.76:19000`

---

## ⚙️ Configuración

### Variables de Entorno

Puedes crear un `.env` en esta carpeta para personalizar:

```bash
TZ=America/Bogota
GENERIC_TIMEZONE=America/Bogota
```

### Volúmenes

- **`portainer_data`**: Almacena preferencias, contraseñas, configuración de Portainer

### Red

- **`portainer-net`**: Red bridge para conectar otros servicios si es necesario

---

## 🔍 Verificar Estado

### Ver si Portainer está corriendo

```bash
docker ps | grep portainer
```

### Ver logs

```bash
docker logs portainer
```

### Ver logs en tiempo real

```bash
docker logs -f portainer
```

### Ver estado de docker-compose

```bash
docker compose ps
```

---

## 📊 Comandos Útiles

### Levantar y ver logs

```bash
docker compose up
```

### Levantar en segundo plano

```bash
docker compose up -d
```

### Detener Portainer

```bash
docker compose stop
```

### Reiniciar Portainer

```bash
docker compose restart
```

### Eliminar Portainer (sin datos)

```bash
docker compose down
```

### Eliminar Portainer (incluyendo datos)

```bash
docker compose down -v
```

---

## 🔐 Seguridad

- Portainer genera automáticamente certificados SSL autofirmados
- Se recomienda cambiar la contraseña en la primera vez
- Los puertos 19000 y 19443 están en localhost (opcional cambiar en `docker-compose.yml`)

---

## ⚠️ Problemas Comunes

### Problema 1: Error "No puede acceder a localhost:19000"

**Solución:**

1. Verifica que Portainer esté corriendo:
   ```bash
   docker compose ps
   ```

2. Si no aparece, levántalo:
   ```bash
   docker compose up -d
   ```

3. Espera 10 segundos y vuelve a intentar

---

### Problema 2: Los Contenedores No Aparecen en Portainer

**Causa:** Portainer está usando el contexto equivocado de Docker.

**Solución:**

```bash
# Ver contexto actual
docker context show

# Cambiar al contexto correcto
docker context use default

# Verificar que cambió
docker context show
```

Debería mostrar: `default`

Luego reinicia Portainer:

```bash
docker restart portainer
```

---

### Problema 3: Los Contenedores Reales Están en Otro Lado

**Problema:** Cuando ejecutas `docker ps -a` sale vacío, pero `sudo docker ps -a` sí muestra contenedores.

**Causa:** Docker daemon es rootful y tu usuario no tiene permisos suficientes.

**Solución:**

```bash
# Ver los contenedores con permisos elevados
sudo docker ps -a

# O agregar tu usuario al grupo docker
sudo usermod -aG docker $USER
newgrp docker

# Después recarga la sesión
docker ps -a
```

---

### Problema 4: Portainer No Inicia

**Problema:** Das `docker compose up -d` pero Portainer no aparece.

**Verificación:**

```bash
# Ver estado
docker compose ps

# Ver logs de errores
docker compose logs portainer

# Ver logs en vivo
docker logs -f portainer
```

**Causa Común:** Puerto 19000 ya está en uso

**Solución:**

Edita `docker-compose.yml`:

```yaml
ports:
  - "19001:9000"    # Cambiar a otro puerto libre
  - "19443:9443"
```

Luego accede a: `http://localhost:19001`

---

### Problema 5: Permisos de Docker Socket

**Problema:** Portainer da error de acceso al socket de Docker.

**Verificación:**

```bash
ls -l /var/run/docker.sock
```

Debería mostrar algo como:
```
srw-rw---- 1 root docker ...
```

**Si hay problema:**

```bash
# Solución temporal
sudo chmod 666 /var/run/docker.sock

# Solución permanente (agregar usuario al grupo docker)
sudo usermod -aG docker $USER
```

---

### Problema 6: Portainer No Se Reinicia Automáticamente

**Problema:** Rebooteaste la máquina y Portainer no está corriendo.

**Verificación:**

```bash
docker ps -a | grep portainer
```

Si muestra `Exited`, significa que no se reinició.

**Solución:**

El `docker-compose.yml` ya tiene `restart: unless-stopped`, así que debería estar bien. Ver logs:

```bash
docker logs portainer
```

Si sigue sin iniciar automáticamente:

```bash
docker compose up -d
```

---

### Problema 7: Error de Contexto "desktop-linux"

**Problema:** Portainer se creó en contexto `desktop-linux` en lugar de `default`.

**Solución Completa:**

```bash
# 1. Ver contextos disponibles
docker context ls

# 2. Cambiar al correcto
docker context use default

# 3. Eliminar Portainer anterior
docker rm -f portainer
docker volume remove portainer_data 2>/dev/null

# 4. Recrear Portainer
cd gestion_docker/
docker compose up -d

# 5. Acceder a http://localhost:19000
```

---

## 🔍 Diagnóstico Completo {#diagnostico}

Si nada funciona, ejecuta esto:

```bash
#!/bin/bash
echo "=== DIAGNÓSTICO DE DOCKER ==="
echo ""
echo "1. Contexto actual:"
docker context show

echo ""
echo "2. Contextos disponibles:"
docker context ls

echo ""
echo "3. Estado de Docker:"
docker ps -a | head -5

echo ""
echo "4. Volúmenes:"
docker volume ls | grep portainer

echo ""
echo "5. Estado de Portainer:"
docker ps -a | grep portainer

echo ""
echo "6. Logs de Portainer:"
docker logs -n 5 portainer

echo ""
echo "7. Socket de Docker:"
ls -l /var/run/docker.sock

echo ""
echo "8. Grupo de usuario:"
groups $USER
```

---

## ✨ Características Principales

✅ **Gestión de Contenedores**
- Ver, crear, parar, reiniciar, eliminar

✅ **Gestión de Volúmenes**
- Listar, crear, eliminar, inspeccionar

✅ **Gestión de Redes**
- Ver redes, conectar/desconectar contenedores

✅ **Gestión de Imágenes**
- Descargar, eliminar, inspeccionar

✅ **Logs en Tiempo Real**
- Ver logs de cualquier contenedor

✅ **Estadísticas**
- CPU, memoria, red de cada contenedor

---

## 🔗 Enlaces Útiles

- [Portainer Official](https://www.portainer.io/)
- [Documentación Portainer](https://docs.portainer.io/)
- [Comunidad Portainer](https://community.portainer.io/)

---

**Última actualización:** 3 de abril de 2026

---

## 📝 Características Principales

✅ **Gestión de Contenedores**
- Ver, crear, parar, reiniciar, eliminar

✅ **Gestión de Volúmenes**
- Listar, crear, eliminar, inspeccionar

✅ **Gestión de Redes**
- Ver redes, conectar/desconectar contenedores

✅ **Gestión de Imágenes**
- Descargar, eliminar, inspeccionar

✅ **Logs en Tiempo Real**
- Ver logs de cualquier contenedor

✅ **Estadísticas**
- CPU, memoria, red de cada contenedor

---

## 🔗 Enlaces Útiles

- [Portainer Official](https://www.portainer.io/)
- [Documentación Portainer](https://docs.portainer.io/)
- [Comunidad Portainer](https://community.portainer.io/)

---

**Última actualización:** 3 de abril de 2026
