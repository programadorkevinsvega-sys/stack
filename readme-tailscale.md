# Tailscale + OVH VPC - Guía de Configuración

Guía para configurar Tailscale con redes privadas OVH (VPC) y múltiples servidores.

---

## 📋 Resumen de Ejecución (Orden Maestro)

| Paso | Ubicación | Script | Acción |
| ---- | --------- | ------ | ------ |
| 1 | VPS | `08_vps_setup_subnet_router.sh` | Configurar Subnet Router |
| 2 | Panel Tailscale | - | Aprobar ruta 10.0.0.0/16 |
| 3 | VPS | `09_vps_nat_persistence.sh` | Configurar NAT persistente |
| 4 | PC Local | `10_local_accept_routes.sh` | Aceptar rutas en cliente |

**Resultado:**
```
[Tu PC] <---> [Túnel Seguro] <---> [VPS Router] <---> [VPC OVH] <---> [Máquinas Privadas]
```

---

## 🌉 Pack "Especialista" (Subnet Router)

Solo **UN** VPS (el que conecta a la VPC) ejecuta estos scripts:

| Script | Descripción |
| ------ | ----------- |
| `08_vps_setup_subnet_router.sh` | Anuncia ruta 10.0.0.0/16 a Tailscale |
| `09_vps_nat_persistence.sh` | NAT/Masquerade para tráfico de retorno |

> ⚠️ **Nota:** No ejecutar en los 4 VPS. Solo uno actúa como puente para evitar conflictos de rutas.

---

## 💻 Scripts Exclusivos de PC Local

| Script | Uso |
| ------ | --- |
| `01_local_ssh_setup.sh` | Inyectar llave SSH desde laptop al VPS (remoto) |
| `10_local_accept_routes.sh` | Aceptar rutas del Subnet Router en tu PC |

---

## 🔄 Estrategias de Conectividad

### Opción A: Mesh Directo (Recomendada)

Los 4 VPS se conectan directamente vía Tailscale usando IPs `100.x.y.z`. No requiere VPC de OVH.

**Ventajas:**
- Tráfico encriptado WireGuard
- Sin configuración de IPs privadas
- Funciona con 200+ cuentas diferentes

**Comando:** Ejecutar `00-install_tailscale.sh` en cada VPS.

---

### Opción B: Subnet Router Único

Un solo VPS actúa como puente a la VPC OVH (rango 10.0.0.0/16).

| Script | Descripción |
| ------ | ----------- |
| `08_vps_setup_subnet_router.sh` | Anunciar 10.0.0.0/16 |
| `09_vps_nat_persistence.sh` | NAT para tráfico de retorno |

---

### Opción C: Múltiples VPCs (IPs /32)

Cuando cada VPS está en una VPC diferente (cuentas OVH separadas).

**Asignación de IPs únicas:**

| VPS | IP Privada OVH |
| --- | -------------- |
| VPS 1 | 10.0.1.1 |
| VPS 2 | 10.0.2.1 |
| VPS 3 | 10.0.3.1 |
| VPS 4 | 10.0.4.1 |

**Configuración por VPS:**

```bash
# VPS 1
tailscale up --advertise-routes=10.0.1.1/32 --accept-routes

# VPS 2
tailscale up --advertise-routes=10.0.2.1/32 --accept-routes

# VPS 3
tailscale up --advertise-routes=10.0.3.1/32 --accept-routes

# VPS 4
tailscale up --advertise-routes=10.0.4.1/32 --accept-routes
```

> **Nota:** `--accept-routes` es obligatorio para que cada VPS vea las rutas de los demás.

---

## 🎯 Entendiendo los CIDR

| Notación | Significado | IPs Totales |
| -------- | ----------- | ------------|
| `10.0.0.0/16` | Rango completo de VPC | 65,536 |
| `10.0.1.1/32` | Una sola IP (el propio servidor) | 1 |

**Regla de oro:** Nunca anunciar el mismo `/16` desde múltiples VPS. Usar `/32` por servidor cuando estén en VPCs diferentes.

---

## 🔧 Comandos de Diagnóstico

```bash
# Ver estado de toda la flota
tailscale status

# Verificar conectividad P2P (Direct vs Relay)
tailscale netcheck

# Hacer ping a un nodo
tailscale ping 100.x.y.z
```

**UDP: true** = Conexión P2P directa disponible  
**UDP: false** = Usando relay DERP (más lento)