# 🪣 MinIO - Buckets, Backups y Políticas de Acceso

**Ubicación:** `database/minio/doc/BUCKETS_BACKUP.md`  
**Propósito:** Guía para crear, gestionar y respaldar buckets en MinIO  
**Última Actualización:** 2026-04-03  
**Estado:** ✅ Operativo

> ⚠️ **Nota Importante:** Las credenciales en MinIO son `KevinsAdmin / Kevins1612522026` (no `minioadmin`). Si obtiene error "Access Denied", use estas credenciales para configurar el alias.

---

## 📋 Resumen Rápido

| Elemento | Valor |
|----------|-------|
| **URL MinIO** | `http://localhost:9000` |
| **Console (UI)** | `http://localhost:9001` |
| **Usuario** | `KevinsAdmin` |
| **Password** | `Kevins1612522026` |
| **Acceso Remoto** | `http://100.121.34.76:9000` |
| **Ruta de Datos** | `/minio/volumes/data` |

---

## 🚀 Inicio Rápido - MinIO

### 1. Verificar que MinIO está activo
```bash
docker ps | grep minio
```

### 2. Acceder a Console UI
```
http://localhost:9001
Usuario: KevinsAdmin
Password: Kevins1612522026
```

### 3. Configurar MinIO Client (mc) - Primera vez
```bash
# IMPORTANTE: Usar KevinsAdmin, no minioadmin
docker exec minio mc alias set local http://localhost:9000 KevinsAdmin Kevins1612522026
```

### 4. Ver buckets existentes
```bash
docker exec minio mc ls local/
```

### 5. Crear bucket
```bash
docker exec minio mc mb local/mi-bucket
```

---

## 🪣 Crear Buckets

### Opción 1: Desde UI Console

1. Accede a `http://localhost:9001`
2. Login con `KevinsAdmin / Kevins1612522026`
3. Click **"Create bucket"**
4. Nombre: `nombre-bucket`
5. Configurar versioning si necesitas (opcional)
6. Click **"Create"**

### Opción 2: Desde Terminal (Recomendado)

#### Bucket Privado (No acceso público)
```bash
docker exec minio mc mb local/bucket-privado
```

#### Bucket Público (Acceso sin credenciales)
```bash
# 1. Crear bucket
docker exec minio mc mb local/bucket-publico

# 2. Hacer público (lectura)
docker exec minio mc anonymous set download local/bucket-publico
```

---

## 🔐 Configurar Políticas de Acceso

### Política: Privado (Default)
```bash
# Solo con credenciales válidas
docker exec minio mc anonymous set private local/mi-bucket
```

### Política: Público (Lectura sin credenciales)
```bash
# Todos pueden descargar
docker exec minio mc anonymous set download local/mi-bucket
```

### Política: Público Completo (Lectura y listado)
```bash
# Todos pueden ver y descargar
docker exec minio mc anonymous set public local/mi-bucket
```

### Política Personalizada (Lectura de carpeta específica)
```bash
# Solo lectura de carpeta "público/"
docker exec minio mc anonymous set-json local/mi-bucket - <<'EOF'
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::mi-bucket/publico/*"
    }
  ]
}
EOF
```

### Ver política actual
```bash
docker exec minio mc anonymous get-json local/mi-bucket
```

### Hacer bucket privado (revocar acceso anónimo)
```bash
docker exec minio mc anonymous set private local/mi-bucket
```

---

## 📂 Estructura Recomendada (No crear buckets separados)

**Importante:** NO crear buckets como `n8n-backups` y `n8n-logs`. Usar carpetas dentro de buckets principales.

✅ **Estructura correcta:**

```bash
local/
├── n8n/                    (Bucket principal)
│   ├── workflows/
│   ├── data/
│   └── backups/
│       ├── daily/
│       ├── weekly/
│       └── monthly/
└── logs/                   (Bucket para logs)
    ├── n8n/
    └── vault/
```

**Crear la estructura:**

```bash
# 1. Crear buckets
docker exec minio mc mb local/n8n
docker exec minio mc mb local/logs

# 2. Crear carpetas en n8n
echo "" | docker exec -i minio mc pipe local/n8n/workflows/.keep
echo "" | docker exec -i minio mc pipe local/n8n/data/.keep
echo "" | docker exec -i minio mc pipe local/n8n/backups/daily/.keep
echo "" | docker exec -i minio mc pipe local/n8n/backups/weekly/.keep
echo "" | docker exec -i minio mc pipe local/n8n/backups/monthly/.keep

# 3. Crear carpetas en logs
echo "" | docker exec -i minio mc pipe local/logs/n8n/.keep
echo "" | docker exec -i minio mc pipe local/logs/vault/.keep

# 4. Verificar estructura
docker exec minio mc ls -r local/
```

---

## 💾 Backups - Exportar Buckets

### Backup Manual Completo (Local)
```bash
# Crear carpeta local para backup
mkdir -p /tmp/minio-backups

# Descargar bucket completo
docker exec minio mc mirror local/mi-bucket /tmp/minio-backups/mi-bucket-$(date +%Y%m%d)

# Verificar
ls -lh /tmp/minio-backups/
```

### Backup a Archivo TAR (Comprimido)
```bash
# Backup comprimido
docker exec minio mc mirror local/mi-bucket /tmp/mi-bucket && \
tar -czf /tmp/minio-backups/mi-bucket-$(date +%Y%m%d).tar.gz /tmp/mi-bucket

# Listar backups
ls -lh /tmp/minio-backups/
```

### Backup Automático Diario (Script)

Crear `backup_minio.sh`:

```bash
#!/bin/bash
# Backup automático de MinIO

BACKUP_DIR="/var/backups/minio"
DATE=$(date +%Y%m%d-%H%M%S)
BUCKET="mi-bucket"

# Crear directorio si no existe
mkdir -p $BACKUP_DIR

# Backup
docker exec minio mc mirror local/$BUCKET $BACKUP_DIR/$BUCKET-$DATE

# Comprimir
tar -czf $BACKUP_DIR/$BUCKET-$DATE.tar.gz -C $BACKUP_DIR $BUCKET-$DATE

# Limpiar carpeta sin comprimir
rm -rf $BACKUP_DIR/$BUCKET-$DATE

# Mantener solo últimos 30 días
find $BACKUP_DIR -name "*.tar.gz" -mtime +30 -delete

echo "✅ Backup completado: $BACKUP_DIR/$BUCKET-$DATE.tar.gz"
```

**Usar:**
```bash
chmod +x backup_minio.sh
./backup_minio.sh

# O programar cron (diario a las 2 AM):
# 0 2 * * * /path/to/backup_minio.sh
```

---

## 📥 Restaurar Backups

### Restaurar desde Archivo Local
```bash
# Si tienes backup en /tmp/minio-backups/mi-bucket-20260403/

# Opción 1: Mirror (sobrescribe)
docker exec minio mc mirror /tmp/minio-backups/mi-bucket-20260403 local/mi-bucket

# Opción 2: Copiar archivos individuales
docker exec minio mc cp /tmp/minio-backups/mi-bucket-20260403/archivo.pdf local/mi-bucket/
```

### Restaurar desde TAR
```bash
# Extraer
tar -xzf /tmp/minio-backups/mi-bucket-20260403.tar.gz -C /tmp/

# Restaurar
docker exec minio mc mirror /tmp/mi-bucket local/mi-bucket
```

### Restaurar Archivo Específico
```bash
docker exec minio mc cp /ruta/local/archivo.txt local/mi-bucket/directorio/
```

---

## 📊 Gestión de Buckets

### Listar Buckets
```bash
docker exec minio mc ls local/
```

### Ver Tamaño de Bucket
```bash
docker exec minio mc du local/mi-bucket
```

### Ver Archivos en Bucket
```bash
# Listar raíz
docker exec minio mc ls local/mi-bucket/

# Listar recursivo
docker exec minio mc ls -r local/mi-bucket/ | head -50
```

### Contar Archivos
```bash
docker exec minio mc ls -r local/mi-bucket/ | wc -l
```

### Buscar Archivos
```bash
# Buscar PDF
docker exec minio mc ls -r local/mi-bucket/ | grep ".pdf"

# Buscar por tamaño > 100MB
docker exec minio mc find local/mi-bucket/ --larger 100M
```

---

## 🗑️ Limpiar Buckets

### Eliminar Archivo Específico
```bash
docker exec minio mc rm local/mi-bucket/archivo.txt
```

### Eliminar Carpeta Completa
```bash
docker exec minio mc rm -r local/mi-bucket/carpeta/
```

### Eliminar Bucket Completo (⚠️ Cuidado)
```bash
# Vaciar bucket primero
docker exec minio mc rm -r local/mi-bucket/*

# Eliminar bucket
docker exec minio mc rb local/mi-bucket
```

### Limpiar Archivos Antiguos
```bash
# Eliminar archivos más viejos de 30 días
docker exec minio mc rm --older-than 30d local/mi-bucket/
```

---

## 📤 Subir/Descargar Archivos

### Subir Archivo
```bash
# Archivo local → MinIO
docker exec minio mc cp /tmp/documento.pdf local/mi-bucket/
```

### Subir Carpeta Entera
```bash
docker exec minio mc mirror /tmp/mi-carpeta local/mi-bucket/datos/
```

### Descargar Archivo
```bash
# MinIO → Local
docker exec minio mc cp local/mi-bucket/documento.pdf /tmp/
```

### Descargar Carpeta Completa
```bash
docker exec minio mc mirror local/mi-bucket /tmp/descarga-minio
```

---

## 🔗 URLs y Acceso Público

### Generar URL Pública Temporal (7 días)
```bash
docker exec minio mc share download local/mi-bucket/archivo.pdf

# Salida:
# http://localhost:9000/mi-bucket/archivo.pdf?X-Amz-Algorithm=...
```

### URL Pública Permanente (si bucket es público)
```bash
# Suponiendo bucket "bucket-publico" es público:
http://localhost:9000/bucket-publico/archivo.pdf
```

### Acceso desde otra máquina
```bash
http://100.121.34.76:9000/mi-bucket/archivo.pdf
```

---

## 🛠️ Comandos Avanzados

### Sincronización Bidireccional (Mirror Continuo)
```bash
# Mantener MinIO y carpeta local sincronizados
docker exec minio mc mirror -w local/mi-bucket /tmp/sync/
```

### Replicate a Otro MinIO
```bash
# Si tienes otro MinIO
docker exec minio mc alias set remote http://otro.minio:9000 user pass

# Replicar
docker exec minio mc mirror local/mi-bucket remote/mi-bucket
```

### Versioning (Historial de Cambios)
```bash
# Habilitar versionado en bucket
docker exec minio mc version enable local/mi-bucket

# Ver versiones de archivo
docker exec minio mc ls --versions local/mi-bucket/archivo.pdf
```

### Lifecycle (Expiración Automática)
```bash
# Eliminar archivos automáticamente después de 90 días
docker exec minio mc ilm import --expiration-days 90 local/mi-bucket
```

---

## 🔐 Usuarios y Acceso

### Crear Usuario MinIO (No admin)
```bash
docker exec minio mc admin user add local usuario123 password123
```

### Asignar Permisos a Usuario
```bash
# Dar acceso solo a carpeta específica
docker exec minio mc admin policy attach local readonlyuser local/mi-bucket/publico
```

### Listar Usuarios
```bash
docker exec minio mc admin user list local
```

### Cambiar Contraseña
```bash
docker exec minio mc admin user change local usuario123 nuevapassword123
```

---

## 📊 Monitoreo y Estadísticas

### Info de MinIO
```bash
docker exec minio mc admin info local
```

### Espacio Usado
```bash
docker exec minio mc du local/
```

### Health Check
```bash
curl http://localhost:9000/minio/health/live
curl http://localhost:9000/minio/health/ready
```

### Logs
```bash
docker logs minio -f --tail 100
```

---

## ✅ Checklist - Configuración Inicial

- [ ] MinIO iniciado: `docker ps | grep minio`
- [ ] Console accesible: `http://localhost:9001` (Credenciales: `KevinsAdmin / Kevins1612522026`)
- [ ] mc alias configurado: `docker exec minio mc alias list`
- [ ] Bucket `n8n` creado: `docker exec minio mc ls local/ | grep n8n`
- [ ] Bucket `logs` creado: `docker exec minio mc ls local/ | grep logs`
- [ ] Carpetas en n8n: workflows/, data/, backups/daily/, weekly/, monthly/
- [ ] Carpetas en logs: n8n/, vault/
- [ ] Buckets configurados como privados: `docker exec minio mc anonymous get local/n8n`
- [ ] Backup automático programado

---

## 🔧 Script Completo - Configuración Inicial

```bash
#!/bin/bash
# setup_minio.sh - Configuración inicial de MinIO

echo "🪣 Configurando MinIO..."

# 0. Limpiar buckets antiguos (opcional)
echo "Limpiando buckets antiguos..."
docker exec minio mc rb --force local/n8n-backups 2>/dev/null || true
docker exec minio mc rb --force local/n8n-logs 2>/dev/null || true
docker exec minio mc rb --force local/bucket-privado 2>/dev/null || true
docker exec minio mc rb --force local/bucket-publico 2>/dev/null || true
docker exec minio mc rb --force local/borrar 2>/dev/null || true

# 1. Configurar alias
echo "Configurando alias..."
docker exec minio mc alias set local http://localhost:9000 KevinsAdmin Kevins1612522026

# 2. Crear buckets principales
echo "Creando buckets..."
docker exec minio mc mb local/n8n 2>/dev/null || echo "✓ n8n existe"
docker exec minio mc mb local/logs 2>/dev/null || echo "✓ logs existe"

# 3. Crear carpetas dentro de n8n
echo "Creando estructura en n8n..."
echo "" | docker exec -i minio mc pipe local/n8n/workflows/.keep 2>/dev/null || true
echo "" | docker exec -i minio mc pipe local/n8n/data/.keep 2>/dev/null || true
echo "" | docker exec -i minio mc pipe local/n8n/backups/daily/.keep 2>/dev/null || true
echo "" | docker exec -i minio mc pipe local/n8n/backups/weekly/.keep 2>/dev/null || true
echo "" | docker exec -i minio mc pipe local/n8n/backups/monthly/.keep 2>/dev/null || true

# 4. Crear carpetas dentro de logs
echo "Creando estructura en logs..."
echo "" | docker exec -i minio mc pipe local/logs/n8n/.keep 2>/dev/null || true
echo "" | docker exec -i minio mc pipe local/logs/vault/.keep 2>/dev/null || true

# 5. Configurar buckets como privados
echo "Configurando permisos..."
docker exec minio mc anonymous set private local/n8n 2>/dev/null || true
docker exec minio mc anonymous set private local/logs 2>/dev/null || true
echo "✓ Buckets configurados como privados"

# 6. Ver resultado
echo ""
echo "✅ MinIO configurado!"
echo "Estructura creada:"
docker exec minio mc ls -r local/ | head -20

# 7. Mostrar URLs
echo ""
echo "🔗 URLs:"
echo "Console: http://localhost:9001"
echo "API: http://localhost:9000"
echo ""
echo "Credenciales:"
echo "Usuario: KevinsAdmin"
echo "Password: Kevins1612522026"
```

**Usar:**
```bash
chmod +x setup_minio.sh
./setup_minio.sh
```

---

## 🚨 Troubleshooting

| Problema | Solución |
|----------|----------|
| `Access Denied` al listar buckets | Usar correcto alias: `docker exec minio mc alias set local http://localhost:9000 KevinsAdmin Kevins1612522026` |
| `mc policy set` comando no reconocido | Usar `mc anonymous set PERMISSION` en su lugar: `mc anonymous set download bucket` |
| `mkdir` comando no existe en mc | Las carpetas se crean automáticamente con los archivos. Usar: `echo "" \| mc pipe local/bucket/folder/.keep` |
| `mc anonymous remove` no funciona | Usar `mc anonymous set private` en su lugar |
| MinIO no inicia | `docker logs minio -f` |
| Puerto 9000/9001 ocupado | `lsof -i :9000` y liberar |
| Bucket no se crea | Verificar permisos de carpeta `/minio/volumes/data/` |
| Bucket ya existe | Es normal, el comando falla pero el bucket existe |
| No puedo ver buckets | Verificar que el alias está configurado: `docker exec minio mc alias list` |
| Crear buckets separados (n8n-backups) | ❌ NO HACER. Usar carpetas dentro del bucket n8n |
| Espacio lleno | Limpiar con `docker exec minio mc rm -r local/bucket/carpeta` |

---

## 📚 Referencias

- **N8N Integration:** `../../n8n/.archive/n8n.md`
- **MinIO Docs:** `https://min.io/docs/minio/`
- **mc Manual:** `https://min.io/docs/minio/linux/reference/minio-mc.html`
- **Monitoreo:** `../doc/MONITOREO_RECURSOS.md`

---

**Archivo:** `database/minio/doc/BUCKETS_BACKUP.md`  
**Versión:** 1.1  
**Estado:** ✅ Actualizado con estructura correcta (2 buckets, carpetas internas)
**Última corrección:** Estructura simplificada: n8n y logs (no crear buckets separados)
