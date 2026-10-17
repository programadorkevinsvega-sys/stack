# 🚀 Supabase Docker - Supabase Completo con Docker

Este proyecto proporciona una **solución completa de Supabase con Docker** que incluye todas las funcionalidades de Supabase: Studio, API Gateway, PostgreSQL, Auth, Storage, Realtime, Functions y más.

## 🎯 Características Principales

- **Supabase Studio**: Dashboard de administración en http://localhost:3000
- **API Gateway (Kong)**: Gateway principal en http://localhost:8000
- **PostgreSQL**: Base de datos principal
- **Auth**: Autenticación y autorización
- **Storage**: Almacenamiento de archivos
- **Realtime**: Actualizaciones en tiempo real
- **Functions**: Edge Functions
- **Monitoreo**: Prometheus + Grafana (opcional)

## 🏗️ Arquitectura del Proyecto

```
┌─────────────────────────────────────────────────────────────┐
│                    Supabase Docker                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
│  │   Supabase     │  │   Kong API      │  │ PostgreSQL  │ │
│  │    Studio      │  │   Gateway       │  │  Database   │ │
│  │ Port: 3000     │  │ Port: 8000      │  │Port: 5432   │ │
│  └─────────────────┘  └─────────────────┘  └─────────────┘ │
│           │                     │                   │       │
│  ┌────────▼─────────┐  ┌────────▼─────────┐  ┌────▼─────┐ │
│  │   Auth API       │  │   Storage API     │  │ Realtime │ │
│  │   Port: 9999     │  │   Port: 5000      │  │Port: 4000│ │
│  └──────────────────┘  └──────────────────┘  └──────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## 🚀 Inicio Rápido

### **1. Configuración Inicial**
```bash
# Copiar archivo de configuración
cp env.example .env

# Editar variables de entorno (opcional)
nano .env
```

### **2. Iniciar Supabase**
```bash
# Supabase básico
make start

# Con replicación de PostgreSQL
make start-replication

# Con clustering de PostgreSQL
make start-clustering

# Con storage replication
make start-storage-replication

# Todo completo (Supabase + Replica + Cluster + Storage)
make start-full
```

### **3. Acceder a Supabase Studio**
- **URL**: http://localhost:3000
- **Dashboard**: Administración completa de Supabase

### **4. Usar la API**
- **API Gateway**: http://localhost:8000
- **REST API**: http://localhost:8000/rest/v1/
- **Auth API**: http://localhost:8000/auth/v1/
- **Storage API**: http://localhost:8000/storage/v1/
- **Realtime API**: http://localhost:8000/realtime/v1/
- **Functions API**: http://localhost:8000/functions/v1/

## 🔄 **Storage Replication & Backup**

### **Storage Replication**
El proyecto incluye **replicación automática de Storage** para backup y alta disponibilidad:

```bash
# Iniciar storage replication
make start-storage-replication

# Ver estado de storage replication
make storage-replication-status

# Conectar a storage master
make storage-connect

# Conectar a storage replica
make storage-replica-connect
```

### **Backup Automático**
```bash
# Ejecutar backup manual
make storage-backup

# Listar backups disponibles
make storage-backup-list

# Restaurar desde backup
make storage-restore-file FILE=storage-backup-20231201-120000.tar.gz
```

### **Configuración de Storage**
```bash
# Variables de entorno para storage replication
STORAGE_REPLICATION_ENABLED=false
STORAGE_REPLICA_PORT=5001
STORAGE_SYNC_INTERVAL=300
STORAGE_BACKUP_ENABLED=true
STORAGE_BACKUP_SCHEDULE=0 2 * * *
STORAGE_BACKUP_RETENTION=7
```

## 🛠️ Comandos Disponibles

### **Comandos Principales**
```bash
make start                    # Iniciar Supabase completo
make start-replication        # Iniciar con replicación PostgreSQL
make start-clustering         # Iniciar con clustering PostgreSQL
make start-storage-replication # Iniciar con storage replication
make start-full              # Iniciar todo (Supabase + Replica + Cluster + Storage)
make stop                    # Parar servicios
make restart                 # Reiniciar servicios
make status                  # Ver estado de servicios
make logs                    # Ver logs de servicios
make studio                  # Abrir Supabase Studio
make api                     # Ver información de API
make clean                   # Limpiar recursos
```

### **Comandos de Replicación**
```bash
make replication-start    # Iniciar replicación
make replication-stop     # Parar replicación
make replication-status  # Estado de replicación
make postgresql-replica-connect # Conectar a réplica
```

### **Comandos de Clustering**
```bash
make clustering-start    # Iniciar clustering
make clustering-stop     # Parar clustering
make clustering-status   # Estado de clustering
make postgresql-cluster-2-connect # Conectar a cluster node 2
make postgresql-cluster-3-connect # Conectar a cluster node 3
```

### **Comandos de Storage**
```bash
make storage-replication-start    # Iniciar storage replication
make storage-replication-stop     # Parar storage replication
make storage-replication-status   # Estado de storage replication
make storage-connect              # Conectar a storage master
make storage-replica-connect      # Conectar a storage replica
make storage-backup               # Ejecutar backup de storage
make storage-backup-list          # Listar backups disponibles
make storage-restore-file FILE=   # Restaurar desde backup
make storage-logs                 # Logs de storage master
make storage-replica-logs         # Logs de storage replica
```

### **Comandos de Servicios**
```bash
make supabase-studio    # Supabase Studio
make supabase-api       # API Gateway
make supabase-db        # PostgreSQL
make supabase-auth      # Auth API
make supabase-storage   # Storage API
make supabase-realtime  # Realtime API
make supabase-functions # Functions API
```

### **Comandos de Desarrollo**
```bash
make dev-start      # Modo desarrollo
make dev-stop       # Parar desarrollo
make monitoring-start # Iniciar monitoreo
make monitoring-stop  # Parar monitoreo
```

## 🔧 Configuración

### **Variables de Entorno**
```bash
# Copiar archivo de ejemplo
cp env.example .env

# Editar configuración
nano .env
```

### **Configuración Principal**
- **API_EXTERNAL_URL**: URL externa de la API
- **SITE_URL**: URL del sitio web
- **JWT_SECRET**: Secreto JWT
- **POSTGRES_PASSWORD**: Contraseña de PostgreSQL

## 📊 Servicios Incluidos

### **Supabase Studio**
- **URL**: http://localhost:3000
- **Funcionalidad**: Dashboard de administración
- **Características**: Gestión de base de datos, autenticación, storage

### **API Gateway (Kong)**
- **URL**: http://localhost:8000
- **Funcionalidad**: Gateway principal de API
- **Características**: Enrutamiento, autenticación, CORS

### **PostgreSQL**
- **Host**: localhost
- **Puerto**: 5432
- **Usuario**: postgres
- **Contraseña**: (configurada en .env)

### **Auth API**
- **URL**: http://localhost:8000/auth/v1/
- **Funcionalidad**: Autenticación y autorización
- **Características**: Login, registro, JWT, OAuth

### **Storage API**
- **URL**: http://localhost:8000/storage/v1/
- **Funcionalidad**: Almacenamiento de archivos
- **Características**: Upload, download, gestión de archivos

### **Realtime API**
- **URL**: http://localhost:8000/realtime/v1/
- **Funcionalidad**: Actualizaciones en tiempo real
- **Características**: WebSockets, suscripciones, eventos

### **Functions API**
- **URL**: http://localhost:8000/functions/v1/
- **Funcionalidad**: Edge Functions
- **Características**: Serverless functions, TypeScript

## 🔒 Seguridad

### **Configuración de Seguridad**
- **JWT**: Tokens JWT para autenticación
- **CORS**: Configuración de CORS
- **API Keys**: Claves de API para acceso
- **PostgreSQL**: Contraseñas seguras

## 🚀 Despliegue en Producción

### **Variables de Producción**
```bash
# Configurar para producción
API_EXTERNAL_URL=https://your-domain.com
SITE_URL=https://your-domain.com
JWT_SECRET=your-production-jwt-secret
POSTGRES_PASSWORD=your-production-postgres-password
```

## 🐛 Solución de Problemas

### **Verificar Servicios**
```bash
make status
make logs
```

### **Reiniciar Servicios**
```bash
make restart
make stop
make start
```

### **Limpiar Recursos**
```bash
make clean
make clean-all
```

## 📚 Documentación Adicional

- [Supabase Documentation](https://supabase.com/docs)
- [API Reference](https://supabase.com/docs/reference)
- [Auth Guide](https://supabase.com/docs/guides/auth)
- [Storage Guide](https://supabase.com/docs/guides/storage)

## 🤝 Contribuir

1. Fork el proyecto
2. Crear rama para feature (`git checkout -b feature/AmazingFeature`)
3. Commit cambios (`git commit -m 'Add some AmazingFeature'`)
4. Push a la rama (`git push origin feature/AmazingFeature`)
5. Abrir Pull Request

## 📄 Licencia

Este proyecto está bajo la Licencia MIT. Ver el archivo [LICENSE](LICENSE) para más detalles.

## 🆘 Soporte

Si tienes problemas o preguntas:

1. Revisar la documentación
2. Verificar logs: `make logs`
3. Verificar estado: `make status`
4. Crear issue en GitHub

---

**¡Disfruta usando Supabase con Docker!** 🚀✨
