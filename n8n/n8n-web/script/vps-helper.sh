#!/bin/bash

# ========================================
# 🔧 Helper Scripts para n8n VPS
# ========================================
# Uso: ./vps-helper.sh <comando> [parámetros]
# Ejemplo: ./vps-helper.sh scale 5

# Colores para salida
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuración
VPS_IP="${VPS_IP:-}"
VPS_USER="${VPS_USER:-}"
VPS_PATH="${VPS_PATH:-/opt/n8n}"
COMPOSE_FILE="docker-compose.vps.yml"

# ========================================
# Funciones
# ========================================

check_vars() {
    if [ -z "$VPS_IP" ] || [ -z "$VPS_USER" ]; then
        echo -e "${RED}❌ ERROR: Variables no configuradas${NC}"
        echo "Ejecuta: export VPS_IP=192.168.1.100 VPS_USER=usuario"
        exit 1
    fi
}

run_remote() {
    ssh "$VPS_USER@$VPS_IP" "cd $VPS_PATH && $1"
}

scale_workers() {
    local count=$1
    if [ -z "$count" ]; then
        echo -e "${RED}❌ Uso: vps-helper.sh scale <número>${NC}"
        exit 1
    fi
    
    echo -e "${YELLOW}⏳ Escalando a $count workers...${NC}"
    run_remote "docker compose -f $COMPOSE_FILE up -d --scale n8n-worker=$count"
    echo -e "${GREEN}✅ Done!${NC}"
    status
}

status() {
    echo -e "${YELLOW}📊 Estado del VPS:${NC}"
    run_remote "docker compose -f $COMPOSE_FILE ps"
}

logs_workers() {
    echo -e "${YELLOW}📋 Logs de workers (Ctrl+C para salir):${NC}"
    run_remote "docker compose -f $COMPOSE_FILE logs -f n8n-worker"
}

logs_redis() {
    echo -e "${YELLOW}📋 Logs de Redis:${NC}"
    run_remote "docker compose -f $COMPOSE_FILE logs redis"
}

logs_postgres() {
    echo -e "${YELLOW}📋 Logs de PostgreSQL:${NC}"
    run_remote "docker compose -f $COMPOSE_FILE logs postgres"
}

restart_all() {
    echo -e "${YELLOW}⏳ Reiniciando servicios...${NC}"
    run_remote "docker compose -f $COMPOSE_FILE restart"
    sleep 5
    status
}

restart_workers() {
    echo -e "${YELLOW}⏳ Reiniciando workers...${NC}"
    run_remote "docker compose -f $COMPOSE_FILE restart n8n-worker"
    sleep 3
    status
}

pull_updates() {
    echo -e "${YELLOW}⏳ Descargando actualizaciones...${NC}"
    run_remote "docker compose -f $COMPOSE_FILE pull"
    echo -e "${YELLOW}⏳ Levantando servicios actualizados...${NC}"
    run_remote "docker compose -f $COMPOSE_FILE up -d"
    sleep 5
    status
}

health_check() {
    echo -e "${YELLOW}🏥 Verificando salud de servicios...${NC}"
    
    echo -e "\n📌 Redis:"
    run_remote "docker compose -f $COMPOSE_FILE exec redis redis-cli ping" && echo -e "${GREEN}✅ PONG${NC}" || echo -e "${RED}❌ No responde${NC}"
    
    echo -e "\n📌 PostgreSQL:"
    run_remote "docker compose -f $COMPOSE_FILE exec postgres psql -U n8n_user -d n8n -c 'SELECT 1'" && echo -e "${GREEN}✅ OK${NC}" || echo -e "${RED}❌ No responde${NC}"
    
    echo -e "\n📌 Workers:"
    run_remote "docker compose -f $COMPOSE_FILE ps n8n-worker" | grep -q "Up" && echo -e "${GREEN}✅ Corriendo${NC}" || echo -e "${RED}❌ No responden${NC}"
}

queue_length() {
    echo -e "${YELLOW}📊 Trabajos en cola de Redis:${NC}"
    run_remote "docker compose -f $COMPOSE_FILE exec redis redis-cli LLEN 'bull:n8n_default:jobs:wait'"
    echo -e "\n${YELLOW}📊 Trabajos activos:${NC}"
    run_remote "docker compose -f $COMPOSE_FILE exec redis redis-cli HGETALL 'bull:n8n_default:id'"
}

db_executions() {
    echo -e "${YELLOW}📊 Estadísticas de ejecuciones en BD:${NC}"
    run_remote "docker compose -f $COMPOSE_FILE exec postgres psql -U n8n_user -d n8n -c \"
    SELECT 
        status, 
        COUNT(*) as count 
    FROM executions 
    GROUP BY status 
    ORDER BY count DESC;
    \""
}

export_db() {
    local filename="n8n_backup_$(date +%Y%m%d_%H%M%S).sql.gz"
    echo -e "${YELLOW}💾 Exportando BD a $filename...${NC}"
    run_remote "docker compose -f $COMPOSE_FILE exec postgres pg_dump -U n8n_user -d n8n | gzip" > "$filename"
    echo -e "${GREEN}✅ Guardado en: ./$filename${NC}"
}

help() {
    cat << EOF
🔧 n8n VPS Helper - Comandos disponibles

CONFIGURACIÓN:
  export VPS_IP=192.168.1.100
  export VPS_USER=usuario_vps
  export VPS_PATH=/opt/n8n (opcional, por defecto)

COMANDOS:
  status                  - Ver estado de todos los servicios
  
  scale <número>          - Escalar a N workers
                           Uso: ./vps-helper.sh scale 5
  
  logs-workers           - Ver logs de workers (tiempo real)
  logs-redis             - Ver logs de Redis
  logs-postgres          - Ver logs de PostgreSQL
  
  restart-all            - Reiniciar todos los servicios
  restart-workers        - Reiniciar solo los workers
  
  pull-updates           - Descarga última versión y actualiza
  
  health                 - Verificar salud de servicios
  
  queue-length           - Ver cantidad de trabajos en cola
  
  db-stats               - Estadísticas de ejecuciones en BD
  
  export-db              - Exportar BD a archivo SQL comprimido
  
  help                   - Mostrar esta ayuda

EJEMPLOS:
  ./vps-helper.sh scale 10
  ./vps-helper.sh logs-workers
  ./vps-helper.sh health
  ./vps-helper.sh db-stats

EOF
}

# ========================================
# Main
# ========================================

if [ -z "$1" ]; then
    help
    exit 0
fi

check_vars

case "$1" in
    status)
        status
        ;;
    scale)
        scale_workers "$2"
        ;;
    logs-workers)
        logs_workers
        ;;
    logs-redis)
        logs_redis
        ;;
    logs-postgres)
        logs_postgres
        ;;
    restart-all)
        restart_all
        ;;
    restart-workers)
        restart_workers
        ;;
    pull-updates)
        pull_updates
        ;;
    health)
        health_check
        ;;
    queue-length)
        queue_length
        ;;
    db-stats)
        db_executions
        ;;
    export-db)
        export_db
        ;;
    help)
        help
        ;;
    *)
        echo -e "${RED}❌ Comando desconocido: $1${NC}"
        help
        exit 1
        ;;
esac
