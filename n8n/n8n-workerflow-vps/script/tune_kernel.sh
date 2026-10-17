#!/bin/bash
set -euo pipefail

# Kernel tuning script optimizado para n8n en Modo Cola
# Especificaciones: 6 vCPU · 12 GB RAM · 1 Gbps Network
# Ejecutar como root: sudo bash tune_kernel_n8n.sh

IFACE=$(ip route | awk '/default/ {print $5}' | head -1)

echo "=== Iniciando Kernel Tuning para n8n en interfaz: $IFACE ==="

cat > /etc/sysctl.d/99-n8n-tuning.conf << 'EOF'
# ==========================================
# 1. ALGORITMO BBR (Acelera Webhooks y APIs)
# ==========================================
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr

# ==========================================
# 2. CONNTRACK (Manejo de conexiones)
# Ajustado a 1 millón para no saturar los 12GB de RAM
# ==========================================
net.netfilter.nf_conntrack_max = 1048576
net.netfilter.nf_conntrack_tcp_timeout_established = 120
net.netfilter.nf_conntrack_tcp_timeout_time_wait = 15
net.netfilter.nf_conntrack_tcp_timeout_close_wait = 10

# ==========================================
# 3. RED: Buffers optimizados para 1 Gbps
# Máximo de 16MB (BDP ideal para 1Gbps), libera RAM para los workers
# ==========================================
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 65536
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.core.rmem_default = 1048576
net.core.wmem_default = 1048576
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216
net.core.netdev_max_backlog = 65536

# ==========================================
# 4. TCP y PUERTOS
# ==========================================
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 15
net.ipv4.tcp_max_tw_buckets = 1048576
net.ipv4.ip_local_port_range = 1024 65535
net.ipv4.tcp_keepalive_time = 60
net.ipv4.tcp_keepalive_intvl = 10
net.ipv4.tcp_keepalive_probes = 3
net.ipv4.tcp_slow_start_after_idle = 0

# ==========================================
# 5. MEMORIA Y DISCO (Optimizando para Node.js/Postgres)
# ==========================================
vm.swappiness = 1
vm.dirty_ratio = 15
vm.dirty_background_ratio = 5

# ==========================================
# 6. LÍMITES DEL SISTEMA
# ==========================================
kernel.pid_max = 4194304
fs.file-max = 2000000
fs.nr_open = 2000000

# ==========================================
# 7. SESIONES E IPC (Inter-Process Communication)
# Optimizado para múltiples workers y conexiones concurrentes
# ==========================================
kernel.sem = 250 32000 100 128
kernel.msgmax = 65536
kernel.msgmnb = 65536
kernel.msgmni = 16384
kernel.shmmax = 68719476736
kernel.shmall = 4294967296
EOF

# Aplicar los cambios en el sistema
sysctl --system

# Límites PAM para sesiones de shell (Archivos abiertos)
grep -q "nofile 1048576" /etc/security/limits.conf 2>/dev/null || cat >> /etc/security/limits.conf << 'EOF'
* soft nofile 1048576
* hard nofile 1048576
root soft nofile 1048576
root hard nofile 1048576
EOF

# RPS: Distribuir interrupciones de red en los 6 CPUs (Hex: 3f)
for rxq in /sys/class/net/$IFACE/queues/rx-*/rps_cpus; do
    echo "3f" > "$rxq" 2>/dev/null || true
done
for rxq in /sys/class/net/$IFACE/queues/rx-*/rps_flow_cnt; do
    echo "4096" > "$rxq" 2>/dev/null || true
done
echo 32768 > /proc/sys/net/core/rps_sock_flow_entries 2>/dev/null || true

# Configurar módulo Conntrack en el Kernel
modprobe nf_conntrack 2>/dev/null || true
if [ -f /sys/module/nf_conntrack/parameters/hashsize ]; then
    echo 262144 > /sys/module/nf_conntrack/parameters/hashsize
fi

# fs.nr_open al máximo permitido en runtime
sysctl -w fs.nr_open=2000000 2>/dev/null || true

echo ""
echo "✅ Kernel tuning aplicado para n8n exitosamente."
echo ""
echo "📊 Verificación del Sistema:"
echo "   TCP BBR Activo : $(sysctl -n net.ipv4.tcp_congestion_control)"
echo "   Conntrack max  : $(cat /proc/sys/net/netfilter/nf_conntrack_max 2>/dev/null || echo N/A)"
echo "   Somaxconn      : $(sysctl -n net.core.somaxconn)"
echo "   File-max       : $(sysctl -n fs.file-max)"
echo "   Swappiness     : $(sysctl -n vm.swappiness)"
echo "======================================================"