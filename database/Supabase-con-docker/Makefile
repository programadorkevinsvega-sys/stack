# Supabase Docker Makefile
# Comandos para gestionar Supabase completo

# Variables por defecto
COMPOSE_CMD := $(shell command -v docker-compose >/dev/null 2>&1 && echo "docker-compose" || echo "docker compose")

help:
	@echo "🚀 Supabase Docker - Comandos Disponibles"
	@echo "========================================"
	@echo "make start          - Iniciar Supabase completo"
	@echo "make start-replication - Iniciar con replicación"
	@echo "make start-clustering - Iniciar con clustering"
	@echo "make start-storage-replication - Iniciar con storage replication"
	@echo "make start-full     - Iniciar todo (Supabase + Replica + Cluster + Storage)"
	@echo "make stop           - Parar servicios"
	@echo "make restart        - Reiniciar servicios"
	@echo "make status         - Ver estado de servicios"
	@echo "make logs           - Ver logs de servicios"
	@echo "make studio         - Abrir Supabase Studio"
	@echo "make api            - Ver información de API"
	@echo "make clean          - Limpiar recursos"
	@echo "make clean-all      - Limpiar todo"

# Iniciar Supabase
start:
	@echo "🚀 Iniciando Supabase completo..."
	@./scripts/start.sh

# Iniciar con replicación
start-replication:
	@echo "🔄 Iniciando Supabase con replicación..."
	@$(COMPOSE_CMD) --profile replication up -d

# Iniciar con clustering
start-clustering:
	@echo "🔄 Iniciando Supabase con clustering..."
	@$(COMPOSE_CMD) --profile clustering up -d

# Iniciar con storage replication
start-storage-replication:
	@echo "🔄 Iniciando Supabase con storage replication..."
	@$(COMPOSE_CMD) --profile storage-replication up -d

# Iniciar todo (Supabase + Replica + Cluster + Storage)
start-full:
	@echo "🚀 Iniciando todo (Supabase + Replica + Cluster + Storage)..."
	@$(COMPOSE_CMD) --profile replication --profile clustering --profile storage-replication up -d

# Parar servicios
stop:
	@echo "🛑 Parando servicios..."
	@$(COMPOSE_CMD) down

# Reiniciar servicios
restart:
	@echo "🔄 Reiniciando servicios..."
	@$(COMPOSE_CMD) restart

# Ver estado de servicios
status:
	@echo "📊 Estado de servicios:"
	@$(COMPOSE_CMD) ps

# Ver logs de servicios
logs:
	@echo "📋 Logs de servicios:"
	@$(COMPOSE_CMD) logs -f

# Abrir Supabase Studio
studio:
	@echo "🌐 Abriendo Supabase Studio..."
	@echo "URL: http://localhost:3000"

# Información de API
api:
	@echo "🔗 Información de API:"
	@echo "  API Gateway: http://localhost:8000"
	@echo "  REST API: http://localhost:8000/rest/v1/"
	@echo "  Auth API: http://localhost:8000/auth/v1/"
	@echo "  Storage API: http://localhost:8000/storage/v1/"
	@echo "  Realtime API: http://localhost:8000/realtime/v1/"
	@echo "  Functions API: http://localhost:8000/functions/v1/"

# Limpiar recursos
clean:
	@echo "🧹 Limpiando recursos..."
	@$(COMPOSE_CMD) down -v
	@docker system prune -f

# Limpiar todo
clean-all:
	@echo "🧹 Limpiando todo..."
	@$(COMPOSE_CMD) down -v --rmi all
	@docker system prune -af

# Comandos específicos de Supabase
supabase-studio:
	@echo "🌐 Supabase Studio: http://localhost:3000"

supabase-api:
	@echo "🔗 API Gateway: http://localhost:8000"

supabase-db:
	@echo "🗄️ PostgreSQL: localhost:5432"

supabase-auth:
	@echo "🔐 Auth API: http://localhost:8000/auth/v1/"

supabase-storage:
	@echo "📁 Storage API: http://localhost:8000/storage/v1/"

supabase-realtime:
	@echo "⚡ Realtime API: http://localhost:8000/realtime/v1/"

supabase-functions:
	@echo "⚙️ Functions API: http://localhost:8000/functions/v1/"

# Comandos de desarrollo
dev-start:
	@echo "🚀 Iniciando en modo desarrollo..."
	@$(COMPOSE_CMD) up -d

dev-stop:
	@echo "🛑 Parando modo desarrollo..."
	@$(COMPOSE_CMD) down

# Comandos de monitoreo
monitoring-start:
	@echo "📊 Iniciando monitoreo..."
	@$(COMPOSE_CMD) --profile monitoring up -d

monitoring-stop:
	@echo "📊 Parando monitoreo..."
	@$(COMPOSE_CMD) --profile monitoring down

# Comandos de replicación
replication-start:
	@echo "🔄 Iniciando replicación..."
	@$(COMPOSE_CMD) --profile replication up -d

replication-stop:
	@echo "🔄 Parando replicación..."
	@$(COMPOSE_CMD) --profile replication down

replication-status:
	@echo "🔄 Estado de replicación:"
	@$(COMPOSE_CMD) --profile replication ps

# Comandos de clustering
clustering-start:
	@echo "🔄 Iniciando clustering..."
	@$(COMPOSE_CMD) --profile clustering up -d

clustering-stop:
	@echo "🔄 Parando clustering..."
	@$(COMPOSE_CMD) --profile clustering down

clustering-status:
	@echo "🔄 Estado de clustering:"
	@$(COMPOSE_CMD) --profile clustering ps

# Comandos de storage replication
storage-replication-start:
	@echo "🔄 Iniciando storage replication..."
	@$(COMPOSE_CMD) --profile storage-replication up -d

storage-replication-stop:
	@echo "🔄 Parando storage replication..."
	@$(COMPOSE_CMD) --profile storage-replication down

storage-replication-status:
	@echo "🔄 Estado de storage replication:"
	@$(COMPOSE_CMD) --profile storage-replication ps

storage-backup:
	@echo "💾 Ejecutando backup de storage..."
	@./scripts/backup-storage.sh

storage-backup-list:
	@echo "📋 Listando backups disponibles:"
	@docker exec supabase-storage-replica ls -la /var/lib/storage/backups/

storage-restore:
	@echo "🔄 Restaurando desde backup (especifica el archivo):"
	@echo "Uso: make storage-restore-file FILE=backup-20231201-120000.tar.gz"

storage-restore-file:
	@echo "🔄 Restaurando desde backup: $(FILE)"
	@./scripts/restore-storage.sh $(FILE)

# Comandos de PostgreSQL
postgresql-connect:
	@echo "🔌 Conectando a PostgreSQL Master..."
	@docker exec -it supabase-db psql -U postgres

postgresql-replica-connect:
	@echo "🔌 Conectando a PostgreSQL Replica..."
	@docker exec -it supabase-db-replica psql -U postgres

postgresql-cluster-2-connect:
	@echo "🔌 Conectando a PostgreSQL Cluster Node 2..."
	@docker exec -it supabase-db-cluster-2 psql -U postgres

postgresql-cluster-3-connect:
	@echo "🔌 Conectando a PostgreSQL Cluster Node 3..."
	@docker exec -it supabase-db-cluster-3 psql -U postgres

# Comandos de Storage
storage-connect:
	@echo "🔌 Conectando a Storage Master..."
	@docker exec -it supabase-storage /bin/bash

storage-replica-connect:
	@echo "🔌 Conectando a Storage Replica..."
	@docker exec -it supabase-storage-replica /bin/bash

storage-logs:
	@echo "📋 Logs de Storage Master:"
	@docker logs supabase-storage

storage-replica-logs:
	@echo "📋 Logs de Storage Replica:"
	@docker logs supabase-storage-replica

# Comandos de información
info:
	@echo "ℹ️ Información del sistema..."
	@echo "Docker Compose: $(COMPOSE_CMD)"
	@echo "Servicios:"
	@$(COMPOSE_CMD) ps
