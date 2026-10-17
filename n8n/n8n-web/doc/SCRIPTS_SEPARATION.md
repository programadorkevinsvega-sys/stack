# 🔄 Separación: Local vs VPS

## 📂 Estructura de Carpetas

```
n8n/
│
├── doc/              ← 📖 DOCUMENTACIÓN LOCAL (estás aquí)
│   ├── README.md
│   ├── QUICK_START.md
│   ├── ARCHITECTURE_SIMPLE.md
│   ├── VARIABLES_EXPLANATION.md
│   ├── SCRIPT_LOCATION.md
│   └── SCRIPTS_SEPARATION.md (este archivo)
│
├── script/           ← 🖥️ SCRIPTS LOCAL
│   ├── verify-config.sh
│   ├── backup.sh
│   ├── vps-helper.sh    (gestiona VPS remotamente)
│   └── QUICK_REFERENCE.sh
│
└── vps/              ← 🚀 DOCUMENTACIÓN Y SCRIPTS VPS
    ├── doc/
    │   ├── README.md (ÍNDICE VPS)
    │   ├── QUICK_START_VPS.md
    │   ├── HEALTH_MONITORING.md
    │   ├── MAINTENANCE_GUIDE.md
    │   ├── SCALING_STRATEGIES.md
    │   └── TROUBLESHOOTING_VPS.md
    │
    └── script/
        ├── health-check.sh
        └── maintenance.sh
```

---

## 📚 Documentación: Local vs VPS

### 📖 Local (`doc/`)

Documentación EXCLUSIVA para tu máquina local:

- **QUICK_START.md** - Setup inicial en PC
- **ARCHITECTURE_SIMPLE.md** - Cómo funciona PC + servicios
- **VARIABLES_EXPLANATION.md** - Variables del `.env` de PC
- **SCRIPT_LOCATION.md** - Cuándo ejecutar scripts de PC

### 📖 VPS (`vps/doc/`)

Documentación EXCLUSIVA para VPS:

- **QUICK_START_VPS.md** - Setup inicial en VPS
- **HEALTH_MONITORING.md** - Monitoreo del VPS
- **MAINTENANCE_GUIDE.md** - Maintenance del VPS
- **SCALING_STRATEGIES.md** - Escalar workers en VPS
- **TROUBLESHOOTING_VPS.md** - Problemas en VPS

---

## 🖥️ Scripts: PC vs VPS

### Scripts de PC (`script/`)

Ejecutados EN tu máquina local:

- **verify-config.sh** - Validar configuración antes de levantar
- **backup.sh** - Hacer backups de BD
- **QUICK_REFERENCE.sh** - Cheat sheet visual
- **vps-helper.sh** - Gestionar VPS remotamente vía SSH

### Scripts de VPS (`vps/script/`)

Ejecutados EN el VPS (o accesibles vía SSH):

- **health-check.sh** - Verificar salud del VPS
- **maintenance.sh** - Tareas de mantenimiento

---

## 🎯 Regla Simple

```
¿Estoy leyendo sobre tu máquina local?      → Lee doc/
¿Estoy leyendo sobre VPS?            → Lee vps/doc/

¿Ejecuto script desde PC?            → Usa script/
¿Ejecuto script en VPS?              → Usa vps/script/
```

---

## ✅ Ventajas de esta Separación

✅ **Claridad**: Sabes exactamente dónde buscar  
✅ **Menos confusión**: No mezclas configuraciones PC vs VPS  
✅ **Escalabilidad**: Puedes copiar `vps/` a múltiples VPS  
✅ **Mantenimiento**: Cada cosa en su lugar  

---

## 📍 Ubicación de Archivos

**Local**: Todo aquí en carpeta principal
```bash
/opt/n8n/
├── docker-compose.yml       ← Local
├── .env                     ← Local
├── script/                  ← Scripts PC
└── doc/                     ← Docs PC
```

**VPS**: Todo en carpeta `vps/`
```bash
/opt/n8n/vps/
├── docker-compose.vps.yml   ← VPS
├── .env.example.vps         ← VPS
├── script/                  ← Scripts VPS
└── doc/                     ← Docs VPS
```

---

## 🔗 Relación entre PC y VPS

Aunque está todo separado, están **conectados**:

```
Tu Máquina Local        VPS Remoto
    ├── docker-compose.yml   ├── docker-compose.vps.yml
    ├── script/              ├── script/
    │   ├── vps-helper.sh ───┼──→ (SSH) Gestión remota
    │   └── backup.sh        │
    └── doc/                 └── doc/
```

**vps-helper.sh** puede gestionar el VPS desde tu PC via SSH.

---

## 🚀 Siguiente Paso

Eres usuario de **tu máquina local**:
- Lee: [QUICK_START.md](QUICK_START.md)

Tienes un **VPS también**:
- Lee: [../vps/doc/QUICK_START_VPS.md](../vps/doc/QUICK_START_VPS.md)

