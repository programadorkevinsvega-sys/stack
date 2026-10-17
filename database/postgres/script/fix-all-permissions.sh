#!/bin/bash
#
# 🔧 FIX-PERMISSIONS - Script maestro para todos los servicios
# Restaura permisos de todos los volúmenes de datos
# Uso: ./fix-all-permissions.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "═══════════════════════════════════════════════════════════════"
echo "  🔧 RESTAURANDO PERMISOS DE TODOS LOS SERVICIOS"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Función para arreglar permisos de un servicio
fix_service_permissions() {
  local service=$1
  local path="$SCRIPT_DIR/volumes/$service"
  
  if [ ! -d "$path" ]; then
    echo "⏭️  $service: No existe el directorio, saltando..."
    return 0
  fi
  
  echo "🔧 $service:"
  
  # Determinar el UID según el servicio
  case $service in
    postgres)
      UID=999
      echo "   👤 UID 999 (postgres)"
      ;;
    mysql)
      UID=999
      echo "   👤 UID 999 (mysql)"
      ;;
    mongodb)
      UID=999
      echo "   👤 UID 999 (mongodb)"
      ;;
    redis)
      UID=999
      echo "   👤 UID 999 (redis)"
      ;;
    rabbitmq)
      UID=999
      echo "   👤 UID 999 (rabbitmq)"
      ;;
    kafka)
      UID=1000
      echo "   👤 UID 1000 (kafka)"
      ;;
    vault)
      UID=100
      echo "   👤 UID 100 (vault)"
      ;;
    minio)
      UID=1000
      echo "   👤 UID 1000 (minio)"
      ;;
    *)
      UID=999
      echo "   👤 UID 999 (default)"
      ;;
  esac
  
  # Cambiar propietario
  sudo chown -R "$UID:$UID" "$path" 2>/dev/null || true
  
  # Permisos de directorios
  sudo find "$path" -type d -exec chmod 700 {} \; 2>/dev/null || true
  
  # Permisos de archivos
  sudo find "$path" -type f -exec chmod 600 {} \; 2>/dev/null || true
  
  # Certificados SSL si existen
  if sudo find "$path" -name "*.key" -o -name "*.crt" | grep -q .; then
    echo "   🔐 Ajustando certificados SSL..."
    sudo find "$path" -name "*.key" -exec chmod 600 {} \; 2>/dev/null || true
    sudo find "$path" -name "*.crt" -exec chmod 644 {} \; 2>/dev/null || true
  fi
  
  echo "   ✅ Hecho"
  echo ""
}

# Arreglar permisos para cada servicio
fix_service_permissions "postgres"
fix_service_permissions "mysql"
fix_service_permissions "mongodb"
fix_service_permissions "redis"
fix_service_permissions "rabbitmq"
fix_service_permissions "kafka"
fix_service_permissions "vault"
fix_service_permissions "minio"

echo "═══════════════════════════════════════════════════════════════"
echo "  ✅ PERMISOS RESTAURADOS CORRECTAMENTE"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Ahora puedes ejecutar:"
echo "  docker compose -f docker-compose.postgres.yml up -d"
echo "  docker compose -f docker-compose.redis.yml up -d"
echo "  docker compose -f docker-compose.mysql.yml up -d"
echo "  ... (y así para cada servicio)"
echo ""
echo "O levantar todos con:"
echo "  docker compose -f docker-compose.postgres.yml -f docker-compose.redis.yml -f docker-compose.mysql.yml ... up -d"
echo ""
