#!/bin/bash
# Script to generate rabbitmq.conf based on RABBITMQ_ENABLE_SSL variable

set -e

RABBITMQ_CONF="/etc/rabbitmq/rabbitmq.conf"
ENABLE_SSL="${RABBITMQ_ENABLE_SSL:-false}"

echo "Generating rabbitmq.conf with SSL_ENABLED=$ENABLE_SSL"

cat > "$RABBITMQ_CONF" <<'EOF'
# RabbitMQ Configuration File
# Generated dynamically for flexible TLS support

# Network and protocol
EOF

if [ "$ENABLE_SSL" = "true" ]; then
    cat >> "$RABBITMQ_CONF" <<'EOF'
# SSL/TLS ENABLED - Primary listener on SSL port 5671
listeners.ssl.default = 5671
listeners.tcp = none

# TLS/SSL Configuration
ssl_options.verify = verify_peer
ssl_options.fail_if_no_peer_cert = false
ssl_options.certfile = /etc/rabbitmq/ssl/server.crt
ssl_options.keyfile = /etc/rabbitmq/ssl/server.key
ssl_options.cacertfile = /etc/rabbitmq/ssl/root.crt

# TLS 1.3 Only
ssl_options.versions.1 = tlsv1.3
ssl_options.honor_cipher_order = true

# Management Plugin (without TLS, localhost only)
management.listener.port = 15672

# Allow plaintext on localhost for testing
listeners.ssl.1 = 5671
listeners.tcp.1 = 127.0.0.1:5672
listeners.tcp.2 = 100.121.34.76:5672
EOF
else
    cat >> "$RABBITMQ_CONF" <<'EOF'
# SSL/TLS DISABLED - Using plain AMQP on port 5672
listeners.tcp.default = 5672
listeners.ssl = none

# Management Plugin
management.listener.port = 15672
management.ssl.port = 15671
EOF
fi

cat >> "$RABBITMQ_CONF" <<'EOF'

# Clustering
cluster_partition_handling = autoheal
cluster_formation.peer_discovery_backend = rabbit_peer_discovery_classic_config

# Memory limits
vm_memory_high_watermark.relative = 0.6

# Performance
channel_max = 2048
connection_max = 50000
heartbeat = 60
EOF

chmod 644 "$RABBITMQ_CONF"
echo "rabbitmq.conf generated successfully at $RABBITMQ_CONF"
