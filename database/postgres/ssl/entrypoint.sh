#!/bin/sh
set -e

# Solo verificar que los certificados existen
if [ ! -f "/ssl/postgres/server.crt" ] || [ ! -f "/ssl/postgres/server.key" ]; then
  echo "ERROR: SSL certificates not found in /ssl/postgres/" >&2
  exit 1
fi

# Dejar que docker-entrypoint.sh maneje todo
exec /usr/local/bin/docker-entrypoint.sh "$@"
