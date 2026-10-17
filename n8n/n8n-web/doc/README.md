# 📚 Documentación Local

**⚠️ IMPORTANTE:** Esta carpeta contiene EXCLUSIVAMENTE documentación para **Local**.

Si necesitas documentación para el **VPS Remoto**, ve a [vps/doc/README.md](../vps/doc/README.md)

---

## 📖 Guías Locales

### 🚀 [QUICK_START.md](QUICK_START.md)
Empieza aquí - Setup en 5 minutos en tu máquina local.
- ✅ Verificación de requisitos
- ✅ Paso a paso inicial
- ✅ Levantar n8n-web y workers locales
- ✅ Acceso a la UI

**Para quién:** Primeros pasos en tu máquina local

---

### 🏗️ [ARCHITECTURE_SIMPLE.md](ARCHITECTURE_SIMPLE.md)
Entiende dónde corre qué cosa en tu arquitectura.
- ✅ Explicación de n8n-web vs workers
- ✅ Dónde corren los servicios externos
- ✅ Cómo se conectan todos
- ✅ Diagrama simplificado

**Para quién:** Necesitas entender qué pasa dónde

---

### 🔑 [VARIABLES_EXPLANATION.md](VARIABLES_EXPLANATION.md)
Referencia completa de todas las variables de entorno.
- ✅ Qué significa cada variable
- ✅ Dónde obtener valores
- ✅ Cuáles son críticas
- ✅ Ejemplos de configuración

**Para quién:** Editando .env y necesitas saber qué poner

---

### 📍 [SCRIPT_LOCATION.md](SCRIPT_LOCATION.md)
Dónde ejecutar cada script en tu máquina local y cuándo.
- ✅ verify-config.sh → verificación antes de deploy
- ✅ backup.sh → backups de BD
- ✅ vps-helper.sh → gestión remota del VPS
- ✅ QUICK_REFERENCE.sh → cheat sheet visual

**Para quién:** Tienes un script y necesitas saber cuándo usarlo

---

### 🔄 [SCRIPTS_SEPARATION.md](SCRIPTS_SEPARATION.md)
Explicación de por qué hay scripts distintos para PC vs VPS.
- ✅ Scripts de tu máquina local (qué hacen)
- ✅ Scripts de VPS (qué hacen y dónde)
- ✅ Cuándo ejecutar cada uno
- ✅ Ejemplos de uso

**Para quién:** Necesitas entender la separación PC/VPS

---

## 🗺️ Orientación Rápida

**¿Qué necesito?**

```
"Acabo de descargar esto" 
  → Lee: QUICK_START.md

"¿Dónde corre qué?" 
  → Lee: ARCHITECTURE_SIMPLE.md

"¿Qué variable es para qué?" 
  → Consulta: VARIABLES_EXPLANATION.md

"¿Cuándo ejecuto verify-config.sh?" 
  → Lee: SCRIPT_LOCATION.md

"¿Por qué hay scripts en PC y en VPS?" 
  → Lee: SCRIPTS_SEPARATION.md
```

---

## 📂 Estructura

```
doc/                      ← TÚ ESTÁS AQUÍ (Local Only)
├── README.md             (Este archivo)
├── QUICK_START.md        (Setup inicial)
├── ARCHITECTURE_SIMPLE.md (Explicación)
├── VARIABLES_EXPLANATION.md (Variables)
├── SCRIPT_LOCATION.md    (Dónde ejecutar scripts)
└── SCRIPTS_SEPARATION.md (PC vs VPS)

Para VPS, mira → ../vps/doc/README.md
```

---

## ✅ Lista de Verificación para tu Máquina Local

Antes de levantar n8n:

- [ ] Leo [QUICK_START.md](QUICK_START.md)
- [ ] Redis externo accesible (verificado)
- [ ] PostgreSQL externo accesible (verificado)
- [ ] Copié `.env.example` a `.env`
- [ ] Edité `.env` con valores correctos
- [ ] Ejecuté `./script/verify-config.sh` ✅
- [ ] Ejecuto: `docker compose up -d`
- [ ] Accedo: `http://localhost:5678`

---

## 🚀 Siguiente Paso

**Para configurar tu máquina local:** Abre [QUICK_START.md](QUICK_START.md)

**Para configurar VPS:** Ve a [../vps/doc/README.md](../vps/doc/README.md)

---

**Nota:** Esta documentación es EXCLUSIVAMENTE para tu máquina local. Nada aquí aplica a VPS. Para el VPS, todos los scripts y guías están en `vps/`.
