#!/bin/bash
set -e

# 🔐 SCRIPT MAESTRO - SETUP TLS AUTOMÁTICO PARA TODOS LOS SERVICIOS
# ================================================================
# 
# Este script:
# 1. Genera certificados CA y servidor para todos los 8 servicios
# 2. Prepara formatos especiales (PEM para MongoDB, PKCS12 para Kafka, symlinks para MinIO)
# 3. Configura permisos correctos (644) para Docker
# 4. ✅ TODO AUTOMÁTICO - sin intervención manual
#
# Uso:
#   bash database/ssl/00-setup-all-tls.sh [IP1,IP2,...]
#
# Ejemplo con IPs recomendadas:
#   bash database/ssl/00-setup-all-tls.sh "100.121.34.76 localhost 127.0.0.1"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DB_DIR="$(dirname "$SCRIPT_DIR")"

# IPs por defecto
IPS="${1:-100.121.34.76 localhost 127.0.0.1}"

echo "🔐 ===== SETUP TLS AUTOMÁTICO PARA TODOS LOS SERVICIOS ====="
echo ""
echo "📍 IPs configuradas: $IPS"
echo ""

SERVICES=("postgres" "redis" "mysql" "mongodb" "rabbitmq" "kafka" "minio" "vault" "nats")

# ============================================================
# PASO 1: Generar CA y Certificados Servidor
# ============================================================
echo "📋 PASO 1: Generando CA y certificados servidor..."
echo ""

for service in "${SERVICES[@]}"; do
  SSL_DIR="$DB_DIR/$service/ssl"
  
  if [ ! -d "$SSL_DIR" ]; then
    echo "⚠️  $service: No existe carpeta ssl, saltando..."
    continue
  fi
  
  echo "🔑 $service:"
  
  # Crear CA si no existe
  if [ ! -f "$SSL_DIR/generales/root.key" ]; then
    echo "  → Generando CA..."
    cd "$SSL_DIR"
    ./01-generate-ca.sh > /dev/null 2>&1
  else
    echo "  → CA ya existe (reutilizando)"
  fi
  
  # Regenerar certificado servidor
  echo "  → Generando certificado servidor..."
  cd "$SSL_DIR"
  rm -f generales/server.* generales/server.ext 2>/dev/null || true
  # Usar parametros para no necesitar input interactivo
  ./02-generate-server-cert.sh $IPS > /dev/null 2>&1
  
  echo "  ✅ Completado"
done

echo ""

# ============================================================
# PASO 2: Preparar formatos especiales
# ============================================================
echo "📋 PASO 2: Preparando formatos especiales..."
echo ""

# MongoDB: Crear PEM
echo "🍃 MongoDB:"
cd "$DB_DIR/mongodb/ssl"
bash prepare-certs.sh > /dev/null 2>&1
echo "  ✅ PEM preparado"

# Kafka: Crear PKCS12
echo "📨 Kafka:"
cd "$DB_DIR/kafka/ssl"
bash prepare-certs.sh > /dev/null 2>&1
echo "  ✅ PKCS12 preparado"

# MinIO: Crear symlinks
echo "🪣 MinIO:"
cd "$DB_DIR/minio/ssl"
bash prepare-certs.sh > /dev/null 2>&1
echo "  ✅ Symlinks preparados"

echo ""

# ============================================================
# PASO 3: Verificar permisos (todos deben ser 644)
# ============================================================
echo "📋 PASO 3: Verificando permisos (todos deben ser 644)..."
echo ""

PERMISOS_OK=true

for service in "${SERVICES[@]}"; do
  CERTS_DIR="$DB_DIR/$service/ssl/generales"
  
  if [ ! -d "$CERTS_DIR" ]; then
    continue
  fi
  
  # Archivos que DEBEN existir
  FILES_TO_CHECK=("root.crt" "root.key" "server.crt" "server.key")
  
  for file in "${FILES_TO_CHECK[@]}"; do
    if [ -f "$CERTS_DIR/$file" ]; then
      PERM=$(stat -c "%a" "$CERTS_DIR/$file")
      if [ "$PERM" != "644" ]; then
        echo "  ⚠️  $service/$file: Permisos $PERM (debería ser 644)"
        chmod 644 "$CERTS_DIR/$file"
        PERMISOS_OK=false
      fi
    fi
  done
done

echo "  ✅ Permisos verificados y corregidos"
echo ""

# ============================================================
# PASO 4: Resumen final
# ============================================================
echo "📋 PASO 4: Resumen de certificados generados"
echo ""

for service in "${SERVICES[@]}"; do
  CERTS_DIR="$DB_DIR/$service/ssl/generales"
  
  if [ ! -d "$CERTS_DIR" ]; then
    echo "  ⚠️  $service: No configurado"
    continue
  fi
  
  COUNT=$(find "$CERTS_DIR" -type f \( -name "*.crt" -o -name "*.key" -o -name "*.pem" -o -name "*.p12" \) | wc -l)
  echo "  ✅ $service: $COUNT archivos"
done

echo ""
echo "🎉 ===== SETUP COMPLETADO ====="
echo ""
echo "📝 Próximos pasos:"
echo "  1. Revisar que TLS esté habilitado en cada .env (si es necesario)"
echo "  2. Ejecutar: docker compose -f [servicio]/docker-compose-[servicio].yml up -d"
echo ""
echo "ℹ️  Todos los certificados están en: [servicio]/ssl/generales/"
echo "ℹ️  Permisos: 644 (legibles por Docker, protegia contra modificación)"
echo ""
