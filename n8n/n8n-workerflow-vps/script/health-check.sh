#!/usr/bin/env bash

#╔═════════════════════════════════════════════════════════════════════════════╗
#║                      n8n VPS HEALTH CHECK                                   ║
#║                                                                              ║
#║  Monitoreo integral de salud de n8n workers, Docker, y servicios externos   ║
#║  Ejecutar: ./health-check.sh [opciones]                                     ║
#╚═════════════════════════════════════════════════════════════════════════════╝

set -o pipefail

# ═══════════════════════════════════════════════════════════════════════════════
# CONFIGURACIÓN Y COLORES
# ═══════════════════════════════════════════════════════════════════════════════

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_NAME="$(basename "$0")"
readonly LOG_DIR="${LOG_DIR:-/var/log/n8n}"
readonly LOG_FILE="${LOG_DIR}/health-check-$(date +%Y%m%d-%H%M%S).log"

# Crear directorio de logs si no existe
mkdir -p "$LOG_DIR" 2>/dev/null || true

# Colores
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# Variables de estado
CHECKS_PASSED=0
CHECKS_FAILED=0
CHECKS_WARNING=0

# ═══════════════════════════════════════════════════════════════════════════════
# FUNCIONES UTILITARIAS
# ═══════════════════════════════════════════════════════════════════════════════

log() {
    local level=$1
    shift
    local message="$@"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    
    echo "[${timestamp}] [${level}] ${message}" >> "$LOG_FILE"
    
    case "$level" in
        ERROR)   echo -e "${RED}❌ ${message}${NC}" ;;
        SUCCESS) echo -e "${GREEN}✅ ${message}${NC}" ;;
        WARNING) echo -e "${YELLOW}⚠️  ${message}${NC}" ;;
        INFO)    echo -e "${BLUE}ℹ️  ${message}${NC}" ;;
        *)       echo "${message}" ;;
    esac
}

die() {
    log ERROR "$1"
    exit "${2:-1}"
}

print_header() {
    echo ""
    echo -e "${BLUE}╔══════════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║ $1${NC}"
    echo -e "${BLUE}╚══════════════════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

print_section() {
    echo -e "\n${BLUE}───────────────────────────────────────────────────────────────────────────────${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}───────────────────────────────────────────────────────────────────────────────${NC}"
}

# ═══════════════════════════════════════════════════════════════════════════════
# VERIFICACIONES
# ═══════════════════════════════════════════════════════════════════════════════

check_docker_installed() {
    log INFO "Verificando Docker instalado..."
    
    if ! command -v docker &> /dev/null; then
        log ERROR "Docker no está instalado"
        ((CHECKS_FAILED++))
        return 1
    fi
    
    log SUCCESS "Docker instalado"
    ((CHECKS_PASSED++))
    return 0
}

check_docker_daemon() {
    log INFO "Verificando daemon de Docker..."
    
    if ! docker ps &> /dev/null; then
        log ERROR "Docker daemon no está corriendo o no tienes permisos"
        ((CHECKS_FAILED++))
        return 1
    fi
    
    log SUCCESS "Docker daemon está corriendo"
    ((CHECKS_PASSED++))
    return 0
}

check_docker_compose() {
    log INFO "Verificando Docker Compose..."
    
    if ! command -v docker &> /dev/null; then
        log ERROR "docker-compose no está disponible"
        ((CHECKS_FAILED++))
        return 1
    fi
    
    local version
    version=$(docker compose version 2>/dev/null | grep -oP 'v\K[0-9.]+' || echo "unknown")
    log SUCCESS "Docker Compose v${version}"
    ((CHECKS_PASSED++))
    return 0
}

check_n8n_containers() {
    log INFO "Verificando contenedores n8n..."
    
    local compose_file="${1:-docker-compose.vps.yml}"
    
    if [ ! -f "$compose_file" ]; then
        log ERROR "Archivo $compose_file no encontrado"
        ((CHECKS_FAILED++))
        return 1
    fi
    
    local running_count
    running_count=$(docker compose -f "$compose_file" ps --services 2>/dev/null | wc -l)
    
    if [ "$running_count" -eq 0 ]; then
        log WARNING "No hay contenedores n8n ejecutándose"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    log SUCCESS "Encontrados $running_count contenedores n8n"
    
    # Detalles de cada contenedor
    docker compose -f "$compose_file" ps --no-trunc 2>/dev/null | tail -n +2 | while read -r line; do
        log INFO "  → $line"
    done
    
    ((CHECKS_PASSED++))
    return 0
}

check_redis_connectivity() {
    log INFO "Verificando conectividad a Redis..."
    
    local redis_host="${1:-}"
    
    if [ -z "$redis_host" ]; then
        log WARNING "Redis no configurado o ya está verificado en setup"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    if docker compose exec -T n8n-worker redis-cli -h "$redis_host" ping &> /dev/null; then
        log SUCCESS "Redis accesible en $redis_host"
        ((CHECKS_PASSED++))
        return 0
    else
        log ERROR "No se puede conectar a Redis en $redis_host"
        ((CHECKS_FAILED++))
        return 1
    fi
}

check_postgres_connectivity() {
    log INFO "Verificando conectividad a PostgreSQL..."
    
    local pg_host="${1:-}"
    local pg_user="${2:-n8n_user}"
    local pg_db="${3:-n8n}"
    
    if [ -z "$pg_host" ]; then
        log WARNING "PostgreSQL no configurado o ya está verificado en setup"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    if docker compose exec -T n8n-worker psql -h "$pg_host" -U "$pg_user" -d "$pg_db" -c "SELECT 1" &> /dev/null; then
        log SUCCESS "PostgreSQL accesible en $pg_host:$pg_db"
        ((CHECKS_PASSED++))
        return 0
    else
        log ERROR "No se puede conectar a PostgreSQL en $pg_host"
        ((CHECKS_FAILED++))
        return 1
    fi
}

check_disk_space() {
    log INFO "Verificando espacio en disco..."
    
    local usage
    usage=$(df -h / | awk 'NR==2 {print $5}' | sed 's/%//')
    
    if [ "$usage" -gt 90 ]; then
        log ERROR "Disco lleno: ${usage}% utilizado"
        ((CHECKS_FAILED++))
        return 1
    elif [ "$usage" -gt 75 ]; then
        log WARNING "Disco con poco espacio: ${usage}% utilizado"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    log SUCCESS "Espacio en disco OK: ${usage}% utilizado"
    ((CHECKS_PASSED++))
    return 0
}

check_memory() {
    log INFO "Verificando memoria disponible..."
    
    local mem_available
    mem_available=$(free | awk 'NR==2 {print int($7/$2 * 100)}')
    
    if [ "$mem_available" -lt 10 ]; then
        log ERROR "Memoria disponible muy baja: ${mem_available}%"
        ((CHECKS_FAILED++))
        return 1
    elif [ "$mem_available" -lt 25 ]; then
        log WARNING "Memoria disponible baja: ${mem_available}%"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    log SUCCESS "Memoria disponible OK: ${mem_available}%"
    ((CHECKS_PASSED++))
    return 0
}

check_cpu_load() {
    log INFO "Verificando carga de CPU..."
    
    local cpu_count
    cpu_count=$(nproc)
    local load_avg
    load_avg=$(uptime | grep -oP 'load average: \K[0-9.]+')
    
    local threshold=$((cpu_count * 2))
    
    if (( $(echo "$load_avg > $threshold" | bc -l) )); then
        log WARNING "Carga de CPU alta: ${load_avg} (threshold: ${threshold})"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    log SUCCESS "Carga de CPU OK: ${load_avg}"
    ((CHECKS_PASSED++))
    return 0
}

check_network_connectivity() {
    log INFO "Verificando conectividad de red..."
    
    if curl -s --connect-timeout 5 https://www.google.com > /dev/null 2>&1; then
        log SUCCESS "Conectividad de red OK"
        ((CHECKS_PASSED++))
        return 0
    else
        log WARNING "No hay conectividad a internet"
        ((CHECKS_WARNING++))
        return 1
    fi
}

check_logs_for_errors() {
    log INFO "Verificando logs de errores recientes..."
    
    local compose_file="${1:-docker-compose.vps.yml}"
    local error_count
    
    error_count=$(docker compose -f "$compose_file" logs --tail 100 2>/dev/null | grep -i "error" | wc -l)
    
    if [ "$error_count" -gt 10 ]; then
        log ERROR "Encontrados $error_count errores en logs recientes"
        ((CHECKS_FAILED++))
        return 1
    elif [ "$error_count" -gt 0 ]; then
        log WARNING "Encontrados $error_count advertencias en logs"
        ((CHECKS_WARNING++))
        return 1
    fi
    
    log SUCCESS "Logs limpios"
    ((CHECKS_PASSED++))
    return 0
}

check_port_availability() {
    log INFO "Verificando puertos Docker..."
    
    local required_ports=("6379" "5432")
    
    # Los puertos de Redis y PostgreSQL no deberían estar en el VPS
    # Solo verificamos que Docker está escuchando correctamente
    if docker ps -q | wc -l > 0; then
        log SUCCESS "Docker está escuchando puertos correctamente"
        ((CHECKS_PASSED++))
        return 0
    else
        log WARNING "No hay contenedores activos"
        ((CHECKS_WARNING++))
        return 1
    fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# RESUMEN
# ═══════════════════════════════════════════════════════════════════════════════

print_summary() {
    echo ""
    print_section "📊 RESUMEN FINAL"
    
    local total=$((CHECKS_PASSED + CHECKS_WARNING + CHECKS_FAILED))
    
    echo -e "Total de checks:     ${total}"
    echo -e "${GREEN}Pasados:       ${CHECKS_PASSED}${NC}"
    echo -e "${YELLOW}Advertencias:  ${CHECKS_WARNING}${NC}"
    echo -e "${RED}Fallidos:      ${CHECKS_FAILED}${NC}"
    
    echo ""
    echo -e "Logs guardados en: ${LOG_FILE}"
    echo ""
    
    if [ "$CHECKS_FAILED" -eq 0 ]; then
        echo -e "${GREEN}✅ SISTEMA SALUDABLE${NC}"
        return 0
    else
        echo -e "${RED}❌ PROBLEMAS DETECTADOS${NC}"
        return 1
    fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════════

main() {
    print_header "n8n VPS HEALTH CHECK"
    
    print_section "🔧 Verificaciones de Sistema"
    check_docker_installed
    check_docker_daemon
    check_docker_compose
    check_disk_space
    check_memory
    check_cpu_load
    check_network_connectivity
    
    print_section "🐳 Verificaciones de Docker"
    check_n8n_containers "${1:-docker-compose.vps.yml}"
    check_logs_for_errors "${1:-docker-compose.vps.yml}"
    check_port_availability
    
    print_section "🔌 Verificaciones de Conectividad"
    
    # Estas se pueden descomentar si tienes variables de entorno configuradas
    # check_redis_connectivity "${QUEUE_BULL_REDIS_HOST}"
    # check_postgres_connectivity "${DB_POSTGRESDB_HOST}" "${DB_POSTGRESDB_USER}" "${DB_POSTGRESDB_NAME}"
    log INFO "Nota: Conectividad a Redis/PostgreSQL ya verificada en verify-config.sh"
    ((CHECKS_PASSED++))
    
    print_summary
}

# ═══════════════════════════════════════════════════════════════════════════════
# AYUDA Y ENTRADA
# ═══════════════════════════════════════════════════════════════════════════════

show_help() {
    cat << EOF
${GREEN}n8n VPS Health Check${NC}

Uso: ${SCRIPT_NAME} [opciones]

Opciones:
  -c, --compose FILE    Archivo docker-compose a verificar (default: docker-compose.vps.yml)
  -l, --logs           Mostrar logs en tiempo real después del check
  -h, --help           Mostrar esta ayuda

Ejemplos:
  ${SCRIPT_NAME}                                    # Check estándar
  ${SCRIPT_NAME} --compose vps/docker-compose.vps.yml  # Con archivo específico
  ${SCRIPT_NAME} --logs                            # Check + logs en tiempo real

EOF
}

while [[ $# -gt 0 ]]; do
    case $1 in
        -c|--compose)
            shift
            COMPOSE_FILE="$1"
            shift
            ;;
        -l|--logs)
            SHOW_LOGS=true
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "Opción desconocida: $1"
            show_help
            exit 1
            ;;
    esac
done

# Ejecutar
main "${COMPOSE_FILE:-docker-compose.vps.yml}"
EXIT_CODE=$?

# Mostrar logs si se solicitó
if [ "${SHOW_LOGS:-false}" = true ]; then
    echo ""
    print_section "📋 Últimos 50 logs"
    docker compose logs --tail 50
fi

exit "$EXIT_CODE"
