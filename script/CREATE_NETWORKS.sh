#!/bin/bash
#
# Create Docker Networks for n8n Stack
# Script para crear las redes Docker necesarias para n8n
#
# Uso: bash CREATE_NETWORKS.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "════════════════════════════════════════════════════════════"
echo "  🌐 Creando Redes Docker para n8n"
echo "════════════════════════════════════════════════════════════"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Función para imprimir con colores
print_status() {
    echo -e "${GREEN}✅${NC} $1"
}

print_error() {
    echo -e "${RED}❌${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ️${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠️${NC} $1"
}

# Verificar que Docker está disponible
if ! command -v docker &> /dev/null; then
    print_error "Docker no está instalado o no está en el PATH"
    exit 1
fi

print_info "Docker encontrado: $(docker --version)"
echo ""

# ============================================
# CREAR STACK-NETWORK (Privada)
# ============================================
echo "┌─ STACK-NETWORK (Red Privada) ──────────────────────────────"
echo "│"

if docker network inspect stack-network > /dev/null 2>&1; then
    print_status "Red 'stack-network' ya existe"
    STACK_EXISTS=true
else
    echo "🔨 Creando red 'stack-network' (bridge)..."
    if docker network create stack-network --driver bridge > /dev/null; then
        print_status "Red 'stack-network' creada exitosamente"
        STACK_EXISTS=false
    else
        print_error "Error al crear 'stack-network'"
        exit 1
    fi
fi

# Mostrar detalles
STACK_ID=$(docker network inspect stack-network --format '{{.ID}}' 2>/dev/null | cut -c1-12)
STACK_IPS=$(docker network inspect stack-network --format '{{range .Containers}}{{.Name}} {{.IPv4Address}}{{"\n"}}{{end}}' 2>/dev/null | wc -l)
print_info "ID: $STACK_ID | Contenedores: $STACK_IPS"
echo "│"
echo "└─────────────────────────────────────────────────────────────"
echo ""

# ============================================
# CREAR PROXY-NETWORK (Pública)
# ============================================
echo "┌─ PROXY-NETWORK (Red Pública) ───────────────────────────────"
echo "│"

if docker network inspect proxy-network > /dev/null 2>&1; then
    print_status "Red 'proxy-network' ya existe"
    PROXY_EXISTS=true
else
    echo "🔨 Creando red 'proxy-network' (bridge)..."
    if docker network create proxy-network --driver bridge > /dev/null; then
        print_status "Red 'proxy-network' creada exitosamente"
        PROXY_EXISTS=false
    else
        print_error "Error al crear 'proxy-network'"
        exit 1
    fi
fi

# Mostrar detalles
PROXY_ID=$(docker network inspect proxy-network --format '{{.ID}}' 2>/dev/null | cut -c1-12)
PROXY_IPS=$(docker network inspect proxy-network --format '{{range .Containers}}{{.Name}} {{.IPv4Address}}{{"\n"}}{{end}}' 2>/dev/null | wc -l)
print_info "ID: $PROXY_ID | Contenedores: $PROXY_IPS"
echo "│"
echo "└─────────────────────────────────────────────────────────────"
echo ""

# ============================================
# VERIFICACIÓN FINAL
# ============================================
echo "┌─ VERIFICACIÓN ──────────────────────────────────────────────"
echo "│"

echo "📊 Redes disponibles:"
docker network ls --filter "name=stack-network\|name=proxy-network" --no-trunc | tail -n +2 | sed 's/^/│   /'

echo "│"
echo "🔗 Contenedores en stack-network:"
STACK_CONTAINERS=$(docker network inspect stack-network --format '{{range .Containers}}{{.Name}}{{"\n"}}{{end}}' | sort)
if [ -z "$STACK_CONTAINERS" ]; then
    echo "│   (ninguno - se conectarán al levantar servicios)"
else
    echo "$STACK_CONTAINERS" | sed 's/^/│   /'
fi

echo "│"
echo "🔗 Contenedores en proxy-network:"
PROXY_CONTAINERS=$(docker network inspect proxy-network --format '{{range .Containers}}{{.Name}}{{"\n"}}{{end}}' | sort)
if [ -z "$PROXY_CONTAINERS" ]; then
    echo "│   (ninguno - se conectarán al levantar proxy)"
else
    echo "$PROXY_CONTAINERS" | sed 's/^/│   /'
fi

echo "│"
echo "└─────────────────────────────────────────────────────────────"
echo ""

# ============================================
# RESUMEN Y PRÓXIMOS PASOS
# ============================================
echo "════════════════════════════════════════════════════════════"
echo "✨ Setup de Redes Completado"
echo "════════════════════════════════════════════════════════════"
echo ""

echo "📋 Próximos Pasos:"
echo ""
echo "1️⃣  Levantar servicios internos (n8n/database/):"
echo "    cd n8n/database/"
echo "    docker-compose -f docker-compose.postgres.yml up -d"
echo "    docker-compose -f docker-compose.redis.yml up -d"
echo "    docker-compose -f docker-compose.rabbitmq.yml up -d"
echo "    # ... (resto de servicios)"
echo ""
echo "2️⃣  Levantar proxy reverso (n8n/proxy/):"
echo "    cd ../proxy/"
echo "    docker-compose -f docker-compose.nginx.yml up -d"
echo "    # O si prefieres: docker-compose -f docker-compose.traefik.yml up -d"
echo ""
echo "3️⃣  Levantar n8n local (n8n/local/):"
echo "    cd ../local/"
echo "    docker-compose up -d"
echo ""
echo "4️⃣  Verificar conectividad:"
echo "    docker network inspect stack-network"
echo "    docker logs n8n-web | grep -i 'connected\|error'"
echo ""
echo "5️⃣  Acceder:"
echo "    http://localhost        (via proxy, público)"
echo "    http://100.121.34.76    (via Tailscale)"
echo ""

print_info "Para más información, ver: CREATE_NETWORK.md"
echo ""

# ============================================
# VERIFICACIÓN DE COMANDOS
# ============================================
echo "🔍 Verificaciones disponibles:"
echo ""
echo "Ver todas las redes:"
echo "  docker network ls"
echo ""
echo "Inspeccionar stack-network:"
echo "  docker network inspect stack-network"
echo ""
echo "Inspeccionar proxy-network:"
echo "  docker network inspect proxy-network"
echo ""
echo "Ver contenedores conectados:"
echo "  docker network inspect stack-network --format '{{range .Containers}}{{.Name}}{{.IPv4Address}}{{\"\\n\"}}{{end}}'"
echo ""

echo "════════════════════════════════════════════════════════════"
echo "✅ Listo para levantar los servicios!"
echo "════════════════════════════════════════════════════════════"
