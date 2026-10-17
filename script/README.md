# Scripts de n8n

Este directorio contiene scripts de configuración y optimización para la plataforma n8n.

## tune_kernel.sh

### 📋 Descripción

Script bash para optimizar el kernel de Linux diseñado específicamente para n8n en modo cola. Realiza ajustes de bajo nivel en el sistema operativo para maximizar el rendimiento de webhooks, conexiones concurrentes y la comunicación inter-procesos.

**Especificaciones de hardware objetivo:**
- 6 vCPU
- 12 GB RAM
- 1 Gbps Network

### 🔧 Requisitos Previos

- Acceso root o capacidad de usar `sudo`
- Sistema Linux basado en kernel moderno (4.0+)
- El script debe ejecutarse en el servidor target

### ⚡ Instalación Rápida

```bash
cd /home/servidor-nitro/servidor-lenovo/stack/n8n/script/
sudo bash tune_kernel.sh
```

### 📊 Optimizaciones Incluidas

#### 1. **Algoritmo BBR** (Aceleración de Webhooks y APIs)
- Congestion control: `bbr` (más eficiente que CUBIC)
- Qdisc: `fq` (Fair Queueing)
- **Beneficio:** Mejor manejo de tráfico de red, menor latencia en webhooks

#### 2. **Conntrack** (Manejo de Conexiones)
- Max connections: 1,048,576
- Timeout establecidas: 120 segundos
- Timeout time_wait: 15 segundos
- **Beneficio:** Manejo eficiente de múltiples conexiones concurrentes sin saturar la RAM

#### 3. **RED** (Buffers de Red)
- Buffers optimizados para 1 Gbps (máximo 16MB)
- `somaxconn`: 65,535 (cola de espera para conexiones)
- `tcp_max_syn_backlog`: 65,536 (SYN backlog)
- `rmem_max/wmem_max`: 16MB (máx read/write memory)
- **Beneficio:** Mejor rendimiento de red sin comprometer memoria disponible para workers

#### 4. **TCP y Puertos**
- `tcp_tw_reuse`: Reutiliza time_wait sockets rápidamente
- `tcp_fin_timeout`: 15 segundos
- Rango de puertos locales: 1024-65535
- Keep-alive optimizado para aplicaciones de larga duración
- **Beneficio:** Mayor disponibilidad de puertos y conexiones más estables

#### 5. **Memoria y Disco**
- Swappiness: 1 (minimiza uso de swap)
- Dirty ratio: 15% foreground, 5% background
- **Beneficio:** Mejor rendimiento de Node.js/PostgreSQL, menos I/O de disco

#### 6. **Límites del Sistema**
- PID max: 4,194,304
- File descriptors max: 2,000,000
- **Beneficio:** Soporta alta concurrencia de procesos

#### 7. **Sesiones e IPC**
- Semaphores: 250 32000 100 128
- Message queue: 65,536 bytes
- Shared memory max: 64GB
- **Beneficio:** Mejor comunicación inter-procesos para múltiples workers

### 🔄 RPS (Receive Packet Steering)

Distribuye interrupciones de red entre los 6 CPUs:
- Cpumask: `3f` (hexadecimal para 6 CPUs)
- RPS flow count: 4,096 por cola de recepción
- Sock flow entries: 32,768

### ✅ Verificación Posterior

El script muestra automáticamente los parámetros aplicados:

```
✅ Kernel tuning aplicado para n8n exitosamente.

📊 Verificación del Sistema:
   TCP BBR Activo : bbr
   Conntrack max  : 1048576
   Somaxconn      : 65535
   File-max       : 2000000
   Swappiness     : 1
```

### 🔍 Verificación Manual

Puedes verificar los cambios en cualquier momento:

```bash
# Ver algoritmo de congestión TCP
sysctl -n net.ipv4.tcp_congestion_control

# Ver máximo de conexiones tracked
cat /proc/sys/net/netfilter/nf_conntrack_max

# Ver límites de file descriptors
cat /proc/sys/fs/file-max

# Ver parámetros de red específicos
sysctl -a | grep -E "(bbr|tcp_|rmem|wmem)"
```

### 📁 Configuraciones Persistentes

El script crea/modifica dos archivos de configuración:

1. **`/etc/sysctl.d/99-n8n-tuning.conf`**
   - Contiene todos los parámetros sysctl
   - Se aplica automáticamente en cada boot

2. **`/etc/security/limits.conf`**
   - Establece límites PAM para file descriptors
   - Aplicado a todas las sesiones de usuario

### 🔄 Cambios en Tiempo de Ejecución

Algunos cambios se aplican también en runtime:
- RPS CPU mask en `/sys/class/net/[IFACE]/queues/rx-*/rps_cpus`
- Conntrack hashsize en `/sys/module/nf_conntrack/parameters/hashsize`

### ⏮️ Rollback (Reversión)

Si necesitas revertir los cambios:

```bash
# Eliminar configuración de sysctl
sudo rm /etc/sysctl.d/99-n8n-tuning.conf
sudo sysctl --system

# Eliminar límites PAM (opcional, requiere editar manualmente)
sudo nano /etc/security/limits.conf

# Reiniciar para aplicar cambios completamente
sudo reboot
```

### ⚠️ Notas Importantes

1. **Requiere Reboot:** Los cambios se aplican inmediatamente, pero para máxima estabilidad se recomienda reiniciar el sistema
2. **Hardware específico:** Los valores están ajustados para 6 vCPU/12GB RAM. Para otro hardware, revisar y ajustar valores
3. **Interfaz de red:** El script detecta automáticamente la interfaz principal (`ip route | awk '/default/ {print $5}'`)
4. **Idempotente:** Es seguro ejecutar el script múltiples veces
5. **Logs:** El script crea configuraciones explícitas (sin logs de cambios anteriores)

### 🐛 Troubleshooting

**Error: "sysctl: setting key failed for file..."**
- Algunos kernels pueden no soportar todos los parámetros
- El script intenta ignorar errores no críticos
- Revisar `/var/log/syslog` para más detalles

**Error: "Permission denied"**
- Asegúrate de ejecutar con `sudo`

**RPS no se aplica:**
- Interfaz de red no encontrada o no es ETH
- Verificar con: `ip route`

### 📚 Referencias

- [Kernel Tuning Performance Wiki](https://wiki.linuxfoundation.org/networking/kernel_network_tuning)
- [BBR Congestion Control](https://www.kernel.org/doc/html/latest/networking/tcp_bbr.html)
- [Conntrack Documentation](https://access.redhat.com/documentation/en-us/red_hat_enterprise_linux/7/html/security_guide/sec-working_with_netfilter)

---

**Última actualización:** 2026-04-03
**Compatible con:** n8n en modo cola con arquitectura específica
