# Tailscale + OVH VPC (10.0.0.0/16) — paso a paso completo

## Objetivo

Conectar tu PC a una red privada de OVH usando **Tailscale** y un **subnet router** en tu VPS, para poder llegar a máquinas internas de la VPC sin exponer puertos públicos.

---

## Escenario

* **VPS / subnet router**: `ubuntu-us-miami-public`
* **IP Tailscale del VPS**: `100.108.233.87`
* **IP privada del VPS en la VPC**: `10.0.0.3`
* **Rango de la VPC**: `10.0.0.0/16`
* **PC cliente**: `servidor-lenovo`

---

## Qué terminó funcionando

* El VPS anunció la ruta privada `10.0.0.0/16` a Tailscale.
* La ruta fue aprobada en el panel de Tailscale.
* Se habilitó `ip_forward` en el VPS.
* Se agregó NAT/MASQUERADE en el VPS para que el tráfico de Tailscale pudiera volver correctamente.
* En tu PC se habilitó `--accept-routes`.
* Finalmente, desde tu PC funcionó `ping 10.0.0.3`.

---

## Paso 1: verificar la IP Tailscale del VPS

En el VPS:

```bash
tailscale ip
```

Salida esperada similar a:

```text
100.108.233.87
fd7a:115c:a1e0::7b33:e957
```

---

## Paso 2: anunciar la red privada desde el VPS

En el VPS:

```bash
tailscale up --advertise-routes=10.0.0.0/16
```

Al principio apareció este warning:

```text
Warning: IP forwarding is disabled, subnet routing/exit nodes will not work.
```

Eso indicaba que faltaba habilitar forwarding.

---

## Paso 3: habilitar IP forwarding en el VPS

### Temporal

```bash
echo 1 > /proc/sys/net/ipv4/ip_forward
```

### Permanente

```bash
echo 'net.ipv4.ip_forward=1' >> /etc/sysctl.conf
sysctl -p
```

### También para IPv6

```bash
echo 'net.ipv6.conf.all.forwarding=1' >> /etc/sysctl.conf
sysctl -p
```

### Verificación

```bash
cat /proc/sys/net/ipv4/ip_forward
```

Debe mostrar:

```text
1
```

---

## Paso 4: volver a anunciar la ruta

Después de habilitar forwarding, en el VPS:

```bash
tailscale up --advertise-routes=10.0.0.0/16
```

Apareció solo un warning de rendimiento:

```text
Warning: UDP GRO forwarding is suboptimally configured on eth0
```

Ese warning **no bloquea** el funcionamiento. Se puede ignorar por ahora.

---

## Paso 5: aprobar la ruta en el panel de Tailscale

En el panel de administración de Tailscale, en la máquina `ubuntu-us-miami-public`, la sección **Subnets** primero mostraba:

```text
Awaiting Approval
10.0.0.0/16
```

Luego se aprobó y quedó así:

```text
Approved
10.0.0.0/16
```

Ese paso es obligatorio. Sin aprobar la ruta, no hay enrutamiento hacia la VPC.

---

## Paso 6: agregar NAT en el VPS

En OVH, lo más práctico fue hacer NAT para que las respuestas hacia clientes Tailscale regresaran correctamente.

En el VPS:

```bash
iptables -t nat -A POSTROUTING -s 100.64.0.0/10 -d 10.0.0.0/16 -j MASQUERADE
```

---

## Paso 7: revisar el estado en el VPS

### Estado de Tailscale

```bash
tailscale status
```

Salida relevante:

```text
100.108.233.87  ubuntu-us-miami-public   ...
100.92.238.1   servidor-lenovo          ... active
100.92.238.1   servidor-nitro           ...
100.71.203.57   ubuntu-us-miami-private  ... offline
```

### Rutas del VPS

```bash
ip route
```

Salida relevante:

```text
default via 10.0.0.1 dev eth0 proto static
10.0.0.0/16 dev eth0 proto kernel scope link src 10.0.0.3
```

### Regla NAT

```bash
iptables -t nat -L -n -v
```

---

## Paso 8: probar conectividad básica desde tu PC al VPS

En tu PC:

```bash
tailscale ping 100.108.233.87
```

Resultado:

```text
pong from ubuntu-us-miami-public (100.108.233.87) via 172.238.215.249:41641 in 63ms
```

Eso confirmó que tu PC sí llegaba al VPS por Tailscale.

---

## Paso 9: primer intento fallido a la red privada

En tu PC, antes de aceptar rutas:

```bash
ping 10.0.0.3
```

Resultado:

```text
100% packet loss
```

Ese fallo pasó porque el cliente todavía no estaba aceptando las rutas anunciadas por el subnet router.

---

## Paso 10: aceptar rutas en tu PC

Primero probaste sin sudo:

```bash
tailscale up --accept-routes
```

Y salió:

```text
Access denied: prefs write access denied
Use 'sudo tailscale up --accept-routes'.
```

Entonces el comando correcto fue:

```bash
sudo tailscale up --accept-routes
```

---

## Paso 11: verificar que ya funcionó

En tu PC:

```bash
ping 10.0.0.3
```

Resultado final:

```text
64 bytes from 10.0.0.3: icmp_seq=1 ttl=64 time=77.4 ms
64 bytes from 10.0.0.3: icmp_seq=2 ttl=64 time=66.9 ms
64 bytes from 10.0.0.3: icmp_seq=3 ttl=64 time=64.8 ms
```

Con eso quedó confirmado que:

* el subnet router funciona,
* el routing funciona,
* el NAT funciona,
* y tu PC ya puede entrar a la VPC privada.

---

## Todos los comandos que sí funcionaron

### En el VPS

```bash
tailscale ip
```

```bash
echo 1 > /proc/sys/net/ipv4/ip_forward
```

```bash
echo 'net.ipv4.ip_forward=1' >> /etc/sysctl.conf
sysctl -p
```

```bash
echo 'net.ipv6.conf.all.forwarding=1' >> /etc/sysctl.conf
sysctl -p
```

```bash
tailscale up --advertise-routes=10.0.0.0/16
```

```bash
iptables -t nat -A POSTROUTING -s 100.64.0.0/10 -d 10.0.0.0/16 -j MASQUERADE
```

```bash
tailscale status
```

```bash
cat /proc/sys/net/ipv4/ip_forward
```

```bash
ip route
```

```bash
iptables -t nat -L -n -v
```

### En tu PC

```bash
tailscale ping 100.108.233.87
```

```bash
sudo tailscale up --accept-routes
```

```bash
ping 10.0.0.3
```

---

## Orden recomendado para repetirlo desde cero

### En el VPS

```bash
echo 1 > /proc/sys/net/ipv4/ip_forward
```

```bash
echo 'net.ipv4.ip_forward=1' >> /etc/sysctl.conf
```

```bash
echo 'net.ipv6.conf.all.forwarding=1' >> /etc/sysctl.conf
```

```bash
sysctl -p
```

```bash
tailscale up --advertise-routes=10.0.0.0/16
```

```bash
iptables -t nat -A POSTROUTING -s 100.64.0.0/10 -d 10.0.0.0/16 -j MASQUERADE
```

### En el panel de Tailscale

* Ir a la máquina `ubuntu-us-miami-public`
* Aprobar subnet `10.0.0.0/16`

### En tu PC

```bash
sudo tailscale up --accept-routes
```

```bash
tailscale ping 100.108.233.87
```

```bash
ping 10.0.0.3
```

---

## Comandos útiles a partir de ahora

### Entrar por SSH a una máquina privada

```bash
ssh usuario@10.0.0.X
```

### Consumir un servicio privado

```bash
curl http://10.0.0.X:PUERTO
```

### Verificar el plano de control del Load Balancer

```bash
curl -i http://10.0.0.3:9095/health
```

Respuesta esperada:

```text
HTTP/1.1 200 OK
{"status":"healthy"}
```

### Ver si Tailscale sigue conectado

```bash
tailscale status
```

---

## Mejora opcional en tu PC

Para no tener que usar `sudo` en comandos de Tailscale:

```bash
sudo tailscale set --operator=$USER
```

---

## Nota sobre persistencia de iptables

La regla NAT que agregaste con `iptables` puede perderse al reiniciar si no la haces persistente.
Una opción común en Ubuntu es instalar `iptables-persistent`:

```bash
apt update
apt install -y iptables-persistent
netfilter-persistent save
```

Si más adelante prefieres una opción más moderna basada en `nftables`, puedes persistir el NAT así:

```bash
apt update
apt install -y nftables
```

Archivo `/etc/nftables.conf` de ejemplo:

```nft
#!/usr/sbin/nft -f

flush ruleset

table ip nat {
  chain postrouting {
    type nat hook postrouting priority srcnat; policy accept;
    ip saddr 100.64.0.0/10 ip daddr 10.0.0.0/16 masquerade
  }
}
```

Luego:

```bash
systemctl enable nftables
systemctl restart nftables
nft list ruleset
```

---

## Resumen final

Terminaste montando esto:

```text
[Tu PC] --Tailscale--> [VPS subnet router 100.108.233.87 / 10.0.0.3] --> [VPC OVH 10.0.0.0/16]
```

Y quedó validado con éxito porque desde tu PC ya respondiste a:

```bash
ping 10.0.0.3
```
