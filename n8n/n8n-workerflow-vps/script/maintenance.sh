#!/usr/bin/env bash

#╔═════════════════════════════════════════════════════════════════════════════╗
#║                      n8n VPS MAINTENANCE                                    ║
#║                                                                              ║
#║  Tareas de mantenimiento: cleanup, logs, estadísticas, rotación de backups  ║
#║  Ejecutar: ./maintenance.sh [comando] [opciones]                            ║
#╚═════════════════════════════════════════════════════════════════════════════╝

set -o pipefail

# ═══════════════════════════════════════════════════════════════════════════════
# CONFIGURACIÓN
# ═══════════════════════════════════════════════════════════════════════════════

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_NAME="$(basename "$0")"
readonly COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.vps.yml}"
readonly LOG_DIR="${LOG_DIR:-/var/log/n8n}"
readonly BACKUP_DIR="${BACKUP_DIR:-/backup/n8n}"
readonly RETENTION_DAYS="${RETENTION_DAYS:-30}"

# Crear directorios si no existen
mkdir -p "$LOG_DIR" "$BACKUP_DIR" 2>/dev/null || true

# Colores
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# ═══════════════════════════════════════════════════════════════════════════════
# FUNCIONES UTILITARIAS
# ═══════════════════════════════════════════════════════════════════════════════

log() {
    local level=$1
    shift
    local message="$@"
    
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

check_docker() {
    if ! command -v docker &> /dev/null; then
        die "Docker no está instalado" 1
    fi
    
    if ! docker ps &> /dev/null; then
        die "Docker daemon no está corriendo" 1
    fi
}

check_compose_file() {
    if [ ! -f "$COMPOSE_FILE" ]; then
        die "Archivo $COMPOSE_FILE no encontrado en $(pwd)" 1
    fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# COMANDOS DE MANTENIMIENTO
# ═══════════════════════════════════════════════════════════════════════════════

# Limpiar imágenes y volúmenes dangling
cleanup_docker() {
    print_header "🧹 Limpieza de Docker"
    
    check_docker
    check_compose_file
    
    log INFO "Limpiando imágenes dangling..."
    local images_removed
    images_removed=$(docker image prune -f --quiet | wc -l)
    log SUCCESS "Eliminadas $images_removed imágenes no utilizadas"
    
    log INFO "Limpiando volúmenes dangling..."
    local volumes_removed
    volumes_removed=$(docker volume prune -f --quiet | wc -l)
    log SUCCESS "Eliminados $volumes_removed volúmenes no utilizados"
    
    log INFO "Limpiando redes dangling..."
    local networks_removed
    networks_removed=$(docker network prune -f --quiet | wc -l)
    log SUCCESS "Eliminadas $networks_removed redes no utilizadas"
    
    log INFO "Limpiando builds dangling..."
    local builds_removed
    builds_removed=$(docker builder prune -f --quiet | wc -l)
    log SUCCESS "Eliminados $builds_removed builds no utilizados"
    
    log SUCCESS "Limpieza completada"
}

# Limpiar logs antiguos
cleanup_logs() {
    local days="${1:-30}"
    
    print_header "🗑️  Limpieza de Logs"
    
    check_docker
    
    log INFO "Eliminando logs más antiguos que $days días..."
    
    if ! command -v journalctl &> /dev/null; then
        log WARNING "journalctl no disponible, omitiendo logs del sistema"
    else
        journalctl --vacuum=time="${days}d" 2>/dev/null || true
        log SUCCESS "Logs del sistema limpiados"
    fi
    
    # Limpiar logs de Docker
    log INFO "Truncando logs de contenedores..."
    for container in $(docker ps -aq); do
        local log_file
        log_file=$(docker inspect -f '{{.LogPath}}' "$container" 2>/dev/null)
        if [ -n "$log_file" ] && [ -f "$log_file" ]; then
            truncate -s 0 "$log_file" 2>/dev/null || true
        fi
    done
    log SUCCESS "Logs de contenedores truncados"
    
    log SUCCESS "Limpieza de logs completada"
}

# Ver estadísticas de contenedores
show_stats() {
    print_header "📊 Estadísticas de Contenedores"
    
    check_docker
    check_compose_file
    
    log INFO "CPU, Memoria y Red:"
    echo ""
    docker stats --no-stream $(docker compose -f "$COMPOSE_FILE" ps -q 2>/dev/null | tr '\n' ' ')
    
    echo ""
    log INFO "Espacio en disco:"
    docker system df
}

# Ver logs de n8n
show_logs() {
    local service="${1:-n8n-worker}"
    local lines="${2:-100}"
    
    print_header "📋 Logs de $service (últimas $lines líneas)"
    
    check_docker
    check_compose_file
    
    docker compose -f "$COMPOSE_FILE" logs --tail "$lines" "$service"
}

# Follow logs en tiempo real
follow_logs() {
    local service="${1:-n8n-worker}"
    
    print_header "🔴 Following logs de $service (Ctrl+C para detener)"
    
    check_docker
    check_compose_file
    
    docker compose -f "$COMPOSE_FILE" logs -f "$service"
}

# Estadísticas de BD
db_stats() {
    print_header "🗄️  Estadísticas de Base de Datos"
    
    # Nota: Requiere acceso directo a PostgreSQL
    # Se assume que las credenciales están en .env
    
    log INFO "Para ejecutar estadísticas de BD, necesitas:"
    log INFO "  1. Tener psql instalado: apt-get install postgresql-client"
    log INFO "  2. Ejecutar directamente en el servidor de BD"
    log INFO ""
    log INFO "Comando: psql -h <DB_HOST> -U <DB_USER> -d <DB_NAME> -c \"SELECT * FROM pg_stat_statements LIMIT 10;\""
}

# Rotación automática de backups
rotate_backups() {
    local days="${1:-$RETENTION_DAYS}"
    
    print_header "🔄 Rotación de Backups"
    
    if [ ! -d "$BACKUP_DIR" ]; then
        log WARNING "Directorio de backups no existe: $BACKUP_DIR"
        return 1
    fi
    
    log INFO "Eliminando backups más antiguos que $days días..."
    
    local removed_count=0
    while IFS= read -r backup_file; do
        rm -f "$backup_file"
        ((removed_count++))
    done < <(find "$BACKUP_DIR" -type f -mtime +"$days" 2>/dev/null)
    
    if [ "$removed_count" -gt 0 ]; then
        log SUCCESS "Eliminados $removed_count backups antiguos"
    else
        log INFO "No hay backups más antiguos que $days días"
    fi
    
    # Mostrar estado actual
    log INFO ""
    log INFO "Estado actual de backups:"
    local total_size
    total_size=$(du -sh "$BACKUP_DIR" 2>/dev/null | awk '{print $1}')
    local backup_count
    backup_count=$(find "$BACKUP_DIR" -type f 2>/dev/null | wc -l)
    
    log INFO "  Backups guardados: $backup_count"
    log INFO "  Tamaño total: $total_size"
}

# Información del sistema
system_info() {
    print_header "🖥️  Información del Sistema"
    
    log INFO "Distribución:"
    lsb_release -d 2>/dev/null || cat /etc/os-release | grep PRETTY_NAME
    
    log INFO ""
    log INFO "CPU:"
    nproc | xargs echo "  Cores:"
    
    log INFO ""
    log INFO "Memoria:"
    free -h | grep "^Mem:" | awk '{print "  Total: " $2 ", Usada: " $3 ", Disponible: " $7}'
    
    log INFO ""
    log INFO "Disco:"
    df -h / | tail -1 | awk '{print "  Tamaño: " $2 ", Usado: " $3 ", Disponible: " $4 ", Uso: " $5}'
    
    log INFO ""
    log INFO "Uptime:"
    uptime -p
    
    log INFO ""
    log INFO "Docker:"
    docker --version
    docker compose --version
}

# Actualizar imágenes de n8n
update_images() {
    print_header "🔄 Actualización de Imágenes"
    
    check_docker
    check_compose_file
    
    log INFO "Descargando últimas imágenes..."
    docker compose -f "$COMPOSE_FILE" pull
    
    log SUCCESS "Nuevas imágenes descargadas"
    log INFO ""
    log INFO "Para aplicar los cambios, ejecutar:"
    log INFO "  docker compose -f $COMPOSE_FILE up -d"
}

# Análisis de uso de recursos
resource_analysis() {
    print_header "📈 Análisis de Recursos"
    
    check_docker
    
    log INFO "Top 10 contenedores por uso de memoria:"
    docker stats --no-stream --format "{{.Container}}\t{{.MemUsage}}" | head -11
    
    log INFO ""
    log INFO "Volúmenes más grandes:"
    docker volume ls --format "{{.Name}}" | while read vol; do
        docker run --rm -v "$vol":/data alpine du -sh /data 2>/dev/null | sed "s|/data|$vol|" || true
    done | sort -hr | head -5
}

# Validación de configuración
validate_config() {
    print_header "✅ Validación de Configuración"
    
    check_docker
    check_compose_file
    
    log INFO "Validando docker-compose.yml..."
    if docker compose -f "$COMPOSE_FILE" config > /dev/null 2>&1; then
        log SUCCESS "Sintaxis válida"
    else
        log ERROR "Sintaxis inválida en docker-compose.yml"
        return 1
    fi
    
    log INFO "Verificando visibilidad de servicios..."
    docker compose -f "$COMPOSE_FILE" ps
    
    log SUCCESS "Validación completada"
}

# ═══════════════════════════════════════════════════════════════════════════════
# AYUDA Y ENTRADA
# ═══════════════════════════════════════════════════════════════════════════════

show_help() {
    cat << EOF
${GREEN}n8n VPS Maintenance${NC}

Uso: ${SCRIPT_NAME} [comando] [opciones]

Comandos Disponibles:

  ${GREEN}Limpieza${NC}
    cleanup          Limpiar imágenes, volúmenes y redes dangling
    clean-logs [N]   Limpiar logs más antiguos que N días (default: 30)
    rotate-backups   Rotar backups antiguos (mantiene últimos 30 días)

  ${GREEN}Monitoreo${NC}
    stats            Mostrar estadísticas de contenedores en tiempo real
    logs [SVC] [N]   Ver últimas N líneas de logs de un servicio (default: 100)
    follow [SVC]     Ver logs en tiempo real (Ctrl+C para parar)
    db-stats         Información sobre estadísticas de base de datos
    system-info      Información del sistema
    resources        Análisis detallado de uso de recursos

  ${GREEN}Mantenimiento${NC}
    update           Descargar últimas imágenes de n8n
    validate         Validar configuración de docker-compose

Ejemplos:
  ${SCRIPT_NAME} cleanup                    # Limpiar Docker
  ${SCRIPT_NAME} clean-logs 7                # Limpiar logs más antiguos de 7 días
  ${SCRIPT_NAME} rotate-backups             # Rotar backups
  ${SCRIPT_NAME} stats                      # Ver estadísticas
  ${SCRIPT_NAME} logs n8n-worker 50        # Ver últimas 50 líneas de worker
  ${SCRIPT_NAME} follow n8n-worker         # Seguir logs de worker
  ${SCRIPT_NAME} system-info               # Ver información del sistema

Variables de Entorno:
  COMPOSE_FILE      Archivo docker-compose (default: docker-compose.vps.yml)
  LOG_DIR           Directorio de logs (default: /var/log/n8n)
  BACKUP_DIR        Directorio de backups (default: /backup/n8n)
  RETENTION_DAYS    Retención de backups en días (default: 30)

Ejemplos con variables:
  COMPOSE_FILE=vps/docker-compose.vps.yml ${SCRIPT_NAME} cleanup
  RETENTION_DAYS=90 ${SCRIPT_NAME} rotate-backups

EOF
}

# ═══════════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════════

main() {
    local command="${1:-help}"
    
    case "$command" in
        cleanup)
            cleanup_docker
            ;;
        clean-logs)
            cleanup_logs "${2:-30}"
            ;;
        stats)
            show_stats
            ;;
        logs)
            show_logs "${2:-n8n-worker}" "${3:-100}"
            ;;
        follow)
            follow_logs "${2:-n8n-worker}"
            ;;
        db-stats)
            db_stats
            ;;
        rotate-backups)
            rotate_backups "${2:-$RETENTION_DAYS}"
            ;;
        system-info)
            system_info
            ;;
        resources)
            resource_analysis
            ;;
        update)
            update_images
            ;;
        validate)
            validate_config
            ;;
        help|-h|--help)
            show_help
            exit 0
            ;;
        *)
            echo -e "${RED}Comando desconocido: $command${NC}"
            echo ""
            show_help
            exit 1
            ;;
    esac
}

# Ejecutar
main "$@"
exit $?
