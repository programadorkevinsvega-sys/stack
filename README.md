# 📦 Hardening Security - Guía de Despliegue

Scripts de hardening para servidor n8n-automatizacion.

---

## 🚀 Opción 1: rsync (Rápida)

```bash
sudo rsync -avz stack/ root@IP:/root/stack/
```

Con barra de progreso:

```bash
rsync -avz --progress stack/ root@IP:/root/stack/
```

---

## 🔒 Opción 2: Tar + SCP (PRO - Más Segura)

Preserva permisos mejor. Ideal para deployments en producción.

### Paso 1: Comprimir

```bash
sudo tar -czpf stack.tar.gz stack
```

### Paso 2: Enviar

```bash
scp stack.tar.gz root@IP:/root/
```

### Paso 3: Descomprimir en destino

```bash
tar -xzpf stack.tar.gz
```

---

## 📋 Orden de Ejecución de Scripts

| # | Script | Descripción |
| - | ------ | ----------- |
| 00 | `00-install_tailscale.sh` | Instalar y configurar Tailscale VPN |
| 01 | `01_local_ssh_setup.sh` | Configuración inicial SSH (antes de hardening) |
| 02 | `02_ssh_hardening.sh` | Hardening SSH Zero-Trust (solo Tailscale + llaves) |
| 03 | `03_firewall_hardening.sh` | UFW + Docker isolation + Cloudflare + SMTP |
| 04 | `04_fail2ban_core.sh` | Fail2Ban core + jaulas (sshd, recidive) |
| 05 | `05_fail2ban_resend.sh` | Alertas email vía API Resend (reemplaza msmtp) |
| 06 | `06_fail2ban_test.sh` | Test automatizado de baneo + Resend API |
| 07 | `07_tune_kernel.sh` | Tuning kernel para 16GB RAM / 6 vCPU / 5 Gbps |

---

## ⚠️ Notas Importantes

- Ejecutar todos los scripts como `root` o con `sudo`
- **Orden crítico:** 00 → 01 → 02 (no te quedes fuera del servidor)
- Editar las variables manuales en cada script antes de ejecutar
- Verificar conectividad SSH después del script `02_ssh_hardening.sh` antes de cerrar la terminal


Para que tus 4 servidores sean clones de seguridad y funcionen dentro de tu búnker digital, el proceso se divide en dos: los scripts que son "obligatorios para todos" y los que son "especiales" según la función del VPS.

Aquí tienes la hoja de ruta clara para tus otros 3 servidores:

🛡️ Pack "Búnker Universal" (Ejecutar si tienes 4 VPS)
Estos scripts son los que blindan la máquina. Cada vez que compres un VPS nuevo, estos deben ir sí o sí para que no tengas "agujeros" en tu red.

00-install_tailscale.sh: Para que el servidor entre en tu red privada.

02_ssh_hardening.sh: Para que nadie entre por IP pública ni con contraseñas.

03_firewall_hardening.sh: Para activar el firewall UFW y proteger los contenedores de Docker.

04_fail2ban_core.sh: Para instalar el motor de bloqueos.

05_fail2ban_resend.sh: Para que cada servidor te avise a tu correo con su propio nombre (ej: VPS-Worker-01, VPS-DB-02).

07_tune_kernel.sh: Para que todos tengan la misma velocidad de red y optimización de RAM.

06_fail2ban_test.sh: Para confirmar que la alerta de ese servidor específico llega bien.
