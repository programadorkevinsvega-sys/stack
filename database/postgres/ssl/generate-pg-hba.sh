#!/bin/bash
# Script to generate pg_hba.conf based on POSTGRES_ENABLE_SSL variable

set -e

PG_HBA_CONF="/var/lib/postgresql/data/pg_hba.conf"
ENABLE_SSL="${POSTGRES_ENABLE_SSL:-false}"

echo "Generating pg_hba.conf with SSL_ENABLED=$ENABLE_SSL"

cat > "$PG_HBA_CONF" <<'EOF'
# TYPE  DATABASE        USER            ADDRESS                 METHOD

# Local connections
local   all             all                                     trust

EOF

if [ "$ENABLE_SSL" = "true" ]; then
    echo "# PostgreSQL configured with MANDATORY SSL/TLS" >> "$PG_HBA_CONF"
    cat >> "$PG_HBA_CONF" <<'EOF'

# IPv4/IPv6 connections require SSL
hostssl all             all             127.0.0.1/32            scram-sha-256
hostssl all             all             172.16.0.0/12           scram-sha-256
hostssl all             all             100.64.0.0/10           scram-sha-256
hostssl all             all             ::1/128                 scram-sha-256
EOF
else
    echo "# PostgreSQL configured WITHOUT mandatory SSL/TLS" >> "$PG_HBA_CONF"
    cat >> "$PG_HBA_CONF" <<'EOF'

# IPv4/IPv6 connections allow both SSL and non-SSL
host  all             all             127.0.0.1/32            scram-sha-256
host  all             all             172.16.0.0/12           scram-sha-256
host  all             all             100.64.0.0/10           scram-sha-256
host  all             all             ::1/128                 scram-sha-256
EOF
fi

chmod 600 "$PG_HBA_CONF"
echo "pg_hba.conf generated successfully at $PG_HBA_CONF"
