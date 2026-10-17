# 📚 VPS Documentation Index

Documentación completa para gestionar n8n workers en VPS remoto.

## 📖 Guías Principales

### 🚀 [QUICK_START_VPS.md](QUICK_START_VPS.md) - Inicio Rápido
**Empieza aquí** si es tu primera vez en el VPS.
- ✅ 10 minutos para estar listo
- ✅ Checklist de requisitos
- ✅ Paso a paso desde cero
- ✅ Verificación de funcionamiento

**Leído por**: Nuevos usuarios, DevOps iniciales

---

### 🏥 [HEALTH_MONITORING.md](HEALTH_MONITORING.md) - Monitoreo de Salud
Usar `health-check.sh` para monitorear la salud integral.
- ✅ 16+ verificaciones automatizadas
- ✅ Interpretación de resultados
- ✅ Alertas y soluciones
- ✅ Cron automático
- ✅ Dashboard de monitoreo

**Ejecutar**: `./vps/script/health-check.sh`

**Leído por**: Administradores, SREs

---

### 🔧 [MAINTENANCE_GUIDE.md](MAINTENANCE_GUIDE.md) - Mantenimiento
Usar `maintenance.sh` para tareas de mantenimiento.
- ✅ 11 comandos disponibles
- ✅ Limpieza, stats, logs, rotación
- ✅ Plan de mantenimiento semanal
- ✅ Automatización con cron
- ✅ Troubleshooting situaciones comunes

**Comandos**: `cleanup`, `stats`, `logs`, `follow`, `rotate-backups`, etc.

**Leído por**: Administradores diarios, Operaciones

---

### 📈 [SCALING_STRATEGIES.md](SCALING_STRATEGIES.md) - Escalabilidad
Cómo escalar workers de forma inteligente.
- ✅ Manual, automático, basado en métricas
- ✅ Tabla de referencia
- ✅ Consideraciones de recursos
- ✅ Límites y opciones
- ✅ Graceful scaling

**Comandos**: `docker compose up -d --scale n8n-worker=N`

**Leído por**: Arquitectos, DevOps, PMs

---

### 🆘 [TROUBLESHOOTING_VPS.md](TROUBLESHOOTING_VPS.md) - Problemas
Solución de problemas comunes y avanzados.
- ✅ Contenedores no inician
- ✅ Conectividad a servicios externos
- ✅ Rendimiento y memory leaks
- ✅ Problemas de disco y recursos
- ✅ y mucho más...

**Leído por**: Todos cuando hay problemas

---

## 🗺️ Mapa de Navegación

```
¿Qué necesito hacer?
│
├─ "Es mi primer día" → QUICK_START_VPS.md
│
├─ "¿Está todo bien?" → HEALTH_MONITORING.md
│
├─ "Limpiar/Mantener" → MAINTENANCE_GUIDE.md
│
├─ "Los workflows lentos" → SCALING_STRATEGIES.md
│
├─ "No funciona algo" → TROUBLESHOOTING_VPS.md
│
└─ "Ver info de sistema" → MAINTENANCE_GUIDE.md (system-info)
```

---

## 🚀 Quick Commands

### Health Check
```bash
./vps/script/health-check.sh
```

### Maintenance
```bash
./vps/script/maintenance.sh stats
./vps/script/maintenance.sh logs n8n-worker 50
./vps/script/maintenance.sh follow n8n-worker
```

### Scaling
```bash
docker compose -f vps/docker-compose.vps.yml up -d --scale n8n-worker=10
```

---

## 📊 Tabla de Contenidos Detallada

| Guía | Secciones | Para Quién |
|------|-----------|-----------|
| **QUICK_START_VPS** | Setup, Variables, Verificación, Monitoreo | Nuevos usuarios |
| **HEALTH_MONITORING** | Checks, Alertas, Cron, Métricas, Dashboard | Todos (monitoreo) |
| **MAINTENANCE_GUIDE** | Cleanup, Logs, Stats, Backups, Cron, Automatización | Administradores |
| **SCALING_STRATEGIES** | Manual, Cron, Auto, Métricas, Recursos, Límites | DevOps |
| **TROUBLESHOOTING_VPS** | 6+ secciones de problemas y soluciones | Debugging |

---

## 🔄 Flujo Típico de Uso

### Día 1: Setup
1. Leer: **QUICK_START_VPS.md**
2. Ejecutar: `./vps/script/health-check.sh`
3. Escalar: `docker compose up -d --scale n8n-worker=5`

### Días 2-7: Monitoreo
1. Revisar: **HEALTH_MONITORING.md**
2. Setupear cron automático
3. Ejecutar: `./vps/script/maintenance.sh stats`

### Semana 2+: Optimización
1. Estudiar: **SCALING_STRATEGIES.md**
2. Analizar: `./vps/script/maintenance.sh resources`
3. Ajustar: `docker compose up -d --scale n8n-worker=<mejor-número>`

### Si hay problemas
1. Ir hasta: **TROUBLESHOOTING_VPS.md**
2. Encontrar síntoma similar
3. Seguir soluciones

---

## 📁 Estructura de Documentación

```
vps/
├── doc/                           ← TÚ ESTÁS AQUÍ
│   ├── README.md                  ← Este archivo
│   ├── QUICK_START_VPS.md         (Setup inicial)
│   ├── HEALTH_MONITORING.md       (Monitoreo)
│   ├── MAINTENANCE_GUIDE.md       (Mantenimiento)
│   ├── SCALING_STRATEGIES.md      (Escalabilidad)
│   └── TROUBLESHOOTING_VPS.md    (Problemas)
│
├── script/
│   ├── health-check.sh
│   └── maintenance.sh
│
├── docker-compose.vps.yml
├── .env.example.vps
└── .env                           (TU FICHERO local)
```

---

## 🆘 Help & Support

### Búsqueda Rápida

```bash
# Buscar en toda la documentation
grep -r "error-que-tengo" vps/doc/

# Ejemplo:
grep -r "Cannot connect to Redis" vps/doc/
```

### Documentación Local

Para información sobre tu máquina local, consultar:
- `/doc/QUICK_START.md`
- `/doc/ARCHITECTURE_SIMPLE.md`
- `/doc/SCRIPT_LOCATION.md`

### Recursos Externos

- **n8n Docs**: https://docs.n8n.io
- **n8n Forum**: https://community.n8n.io
- **GitHub Issues**: https://github.com/n8n-io/n8n/issues
- **Discord**: https://discord.n8n.io

---

## 📝 Tips de Lectura

1. **Primera vez**: Lee QUICK_START_VPS.md completo
2. **Luego**: Guarda HEALTH_MONITORING.md como referencia
3. **Diario**: Skim MAINTENANCE_GUIDE.md
4. **Cuando escalas**: Lee SCALING_STRATEGIES.md
5. **Problemas**: Busca en TROUBLESHOOTING_VPS.md

---

## ✨ Actualización de Documentación

Estas guías se actualizan cuando:
- Se descubren nuevos problemas
- Hay cambios en n8n
- Se aprenden nuevas mejores prácticas

**Última actualización**: Marzo 23, 2026

---

**¡Listo para gestionar tu VPS! 🚀**
