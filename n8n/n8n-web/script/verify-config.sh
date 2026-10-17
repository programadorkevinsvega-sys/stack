#!/bin/bash

# 🔍 Script de Verificación de Configuración n8n
# Verifica que todas las variables críticas están configuradas correctamente

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🔍 Verificación de Configuración n8n${NC}\n"

# Check if .env exists
if [ ! -f .env ]; then
    echo -e "${RED}❌ Error: Archivo .env no encontrado${NC}"
    echo "   Copia: cp .env.example .env"
    exit 1
fi

echo -e "${YELLOW}📋 Comprobando variables en .env...${NC}\n"

# Variables críticas
REQUIRED_VARS=(
    "N8N_ENCRYPTION_KEY"
    "QUEUE_BULL_REDIS_HOST"
    "QUEUE_BULL_REDIS_PORT"
    "DB_POSTGRESDB_HOST"
    "DB_POSTGRESDB_PORT"
    "DB_POSTGRESDB_USER"
    "DB_POSTGRESDB_PASSWORD"
    "DB_TYPE"
    "N8N_HOST"
    "GENERIC_TIMEZONE"
)

MISSING=0

for var in "${REQUIRED_VARS[@]}"; do
    if grep -q "^$var=" .env 2>/dev/null; then
        VALUE=$(grep "^$var=" .env | cut -d'=' -f2)
        if [ -z "$VALUE" ]; then
            echo -e "${RED}❌ $var${NC} - Definida pero vacía"
            MISSING=$((MISSING + 1))
        else
            # Mostrar valor ocultando datos sensibles
            if [[ "$var" == *"PASSWORD"* ]] || [[ "$var" == *"KEY"* ]]; then
                echo -e "${GREEN}✅ $var${NC} - ${VALUE:0:10}***"
            else
                echo -e "${GREEN}✅ $var${NC} - $VALUE"
            fi
        fi
    else
        echo -e "${RED}❌ $var${NC} - No definida"
        MISSING=$((MISSING + 1))
    fi
done

echo ""

# Verificar que no hay valores por defecto
echo -e "${YELLOW}⚠️  Comprobando valores por defecto (deben ser editados)...${NC}\n"

if grep -q "IP_DEL_VPS\|PON_AQUI\|tu_password\|GeneraUnaClaveSecreta" .env 2>/dev/null; then
    echo -e "${RED}❌ Hay valores por defecto sin editar:${NC}"
    grep "IP_DEL_VPS\|PON_AQUI\|tu_password\|GeneraUnaClaveSecreta" .env || true
    MISSING=$((MISSING + 1))
else
    echo -e "${GREEN}✅ Sin valores por defecto${NC}"
fi

echo ""

# Verificar que N8N_ENCRYPTION_KEY es fuerte
echo -e "${YELLOW}🔐 Comprobando solidez de N8N_ENCRYPTION_KEY...${NC}\n"

KEY=$(grep "^N8N_ENCRYPTION_KEY=" .env | cut -d'=' -f2)
KEY_LEN=${#KEY}

if [ "$KEY_LEN" -lt 20 ]; then
    echo -e "${RED}❌ Clave muy corta ($KEY_LEN caracteres, mínimo 20)${NC}"
    echo "   Generar con: openssl rand -base64 32"
    MISSING=$((MISSING + 1))
elif [ "$KEY_LEN" -lt 32 ]; then
    echo -e "${YELLOW}⚠️  Clave aceptable pero podría ser más fuerte ($KEY_LEN caracteres)${NC}"
else
    echo -e "${GREEN}✅ Clave fuerte ($KEY_LEN caracteres)${NC}"
fi

echo ""

# Verificar conectividad a servicios externos
echo -e "${YELLOW}📡 Comprobando conectividad a servicios externos...${NC}\n"

REDIS_HOST=$(grep "^QUEUE_BULL_REDIS_HOST=" .env | cut -d'=' -f2)
REDIS_PORT=$(grep "^QUEUE_BULL_REDIS_PORT=" .env | cut -d'=' -f2 | tr -d ' ')
DB_HOST=$(grep "^DB_POSTGRESDB_HOST=" .env | cut -d'=' -f2)
DB_PORT=$(grep "^DB_POSTGRESDB_PORT=" .env | cut -d'=' -f2 | tr -d ' ')

# Redis
if command -v redis-cli &> /dev/null; then
    if redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping &>/dev/null; then
        echo -e "${GREEN}✅ Redis${NC} accesible en $REDIS_HOST:$REDIS_PORT"
    else
        echo -e "${RED}❌ Redis${NC} NO accesible en $REDIS_HOST:$REDIS_PORT"
        MISSING=$((MISSING + 1))
    fi
else
    echo -e "${YELLOW}⚠️  redis-cli no instalado (salta verificación)${NC}"
    echo "   Instala redis-tools: apt install redis-tools"
fi

# PostgreSQL
if command -v psql &> /dev/null; then
    DB_USER=$(grep "^DB_POSTGRESDB_USER=" .env | cut -d'=' -f2)
    DB_NAME=$(grep "^DB_POSTGRESDB_DATABASE=" .env | cut -d'=' -f2)
    DB_PASS=$(grep "^DB_POSTGRESDB_PASSWORD=" .env | cut -d'=' -f2)
    
    if PGPASSWORD="$DB_PASS" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT 1" &>/dev/null; then
        echo -e "${GREEN}✅ PostgreSQL${NC} accesible en $DB_HOST:$DB_PORT"
    else
        echo -e "${RED}❌ PostgreSQL${NC} NO accesible en $DB_HOST:$DB_PORT"
        MISSING=$((MISSING + 1))
    fi
else
    echo -e "${YELLOW}⚠️  psql no instalado (salta verificación)${NC}"
    echo "   Instala postgresql-client: apt install postgresql-client"
fi

echo ""

# Verificar Docker
echo -e "${YELLOW}🐋 Comprobando Docker...${NC}\n"

if command -v docker &> /dev/null; then
    echo -e "${GREEN}✅ Docker${NC} instalado"
else
    echo -e "${RED}❌ Docker${NC} NO instalado"
    MISSING=$((MISSING + 1))
fi

if command -v docker-compose &> /dev/null; then
    echo -e "${GREEN}✅ Docker Compose${NC} instalado"
else
    echo -e "${RED}❌ Docker Compose${NC} NO instalado"
    MISSING=$((MISSING + 1))
fi

echo ""

# Verificar acceso a internet
echo -e "${YELLOW}🌐 Comprobando acceso a internet...${NC}\n"

if command -v curl &> /dev/null; then
    if curl -s --connect-timeout 3 https://www.google.com >/dev/null 2>&1; then
        echo -e "${GREEN}✅ Acceso a internet${NC} disponible"
    else
        echo -e "${RED}❌ Sin acceso a internet${NC} (requerido para descargas)"
        MISSING=$((MISSING + 1))
    fi
fi

echo ""

# Resumen final
echo "════════════════════════════════════════════════════════"

if [ $MISSING -eq 0 ]; then
    echo -e "${GREEN}✅ TODAS LAS VERIFICACIONES PASARON${NC}"
    echo -e "${GREEN}Está listo para ejecutar: docker compose up -d${NC}"
    exit 0
else
    echo -e "${RED}❌ $MISSING PROBLEMAS ENCONTRADOS${NC}"
    echo -e "${YELLOW}Soluciona los errores anteriores y vuelve a ejecutar este script${NC}"
    exit 1
fi
