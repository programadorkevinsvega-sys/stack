# 📊 Monitoreo de Recursos - n8n (General)

Documentación general para monitorear n8n en cualquier configuración (Web, All-in-One, VPS Workers).

---

## 1. Acceso Rápido por Configuración

### ¿Qué Configuración Usas?

- **n8n-web**: UI completa, una máquina, desarrollo/testing
  → Ver: [n8n-web/doc/MONITOREO_RECURSOS.md](../n8n-web/doc/MONITOREO_RECURSOS.md)

- **n8n-all-basico**: Stack completo en una máquina (n8n + PostgreSQL + Redis)
  → Ver: [n8n-all-basico/doc/MONITOREO_RECURSOS.md](../n8n-all-basico/doc/MONITOREO_RECURSOS.md)

- **n8n-workerflow-vps**: Distribuido (VPS Core + PC Workers)
  → Ver: [n8n-workerflow-vps/doc/MONITOREO_RECURSOS.md](../n8n-workerflow-vps/doc/MONITOREO_RECURSOS.md)

---

## 2. Comandos Universales

### Health Check (aplica a cualquier config)
```bash
# Basic health
curl -s http://localhost:5678/health | jq '.status'

# Full health
curl -s http://localhost:5678/health | jq
```

### Ver logs
```bash
docker logs n8n -f --tail 100
```

### Reiniciar n8n
```bash
docker restart n8n
```

### Ver estado del contenedor
```bash
docker ps | grep n8n
```

---

## 3. Monitoreo de Performance General

### CPU en tiempo real
```bash
docker stats n8n --format "table {{.Container}}\t{{.CPUPerc}}" --no-stream
```

### Memoria en tiempo real
```bash
docker stats n8n --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}" --no-stream
```

### Red (I/O)
```bash
docker stats n8n --format "table {{.Container}}\t{{.NetIO}}" --no-stream
```

### Disco
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep n8n
```

---

## 4. Workflows y Ejecuciones

### Ver workflows
```bash
curl -s http://localhost:5678/api/v1/workflows | jq '.data[] | {id, name, active}'
```

### Ejecuciones recientes
```bash
curl -s http://localhost:5678/api/v1/executions?limit=10 | jq '.data[] | {id, status, startedAt}'
```

### Ejecuciones en progreso
```bash
curl -s http://localhost:5678/api/v1/executions?status=running | jq '.data | length'
```

### Búsqueda de errores
```bash
docker logs n8n | grep -i error | tail -20
```

---

## 5. APIs Principales

### Listado de endpoints disponibles
```bash
# Health & Status
curl http://localhost:5678/health

# Workflows
curl http://localhost:5678/api/v1/workflows

# Executions
curl http://localhost:5678/api/v1/executions

# Nodes (plugins)
curl http://localhost:5678/api/v1/nodes

# Credentials (requiere token)
curl -H "X-N8N-AUTH: $TOKEN" http://localhost:5678/api/v1/credentials
```

---

## 6. Alertas Críticas

### n8n NO responde
```bash
curl -s http://localhost:5678/health | jq -e '.status' > /dev/null || echo "⚠️  N8N CAÍDO"
```

### CPU muy alta
```bash
docker stats n8n --no-stream | awk '$3 > "80.00%" {print "⚠️  CPU ALTA: " $3}'
```

### Memoria casi llena
```bash
docker stats n8n --no-stream | awk '{
  gsub(/[GMiB%]/,"");
  if ($4 > 75) print "⚠️  MEMORIA CRÍTICA: " $4 "%"
}'
```

### Ejecuciones atoradas
```bash
RUNNING=$(curl -s http://localhost:5678/api/v1/executions?status=running 2>/dev/null | jq '.data | length')
[ $RUNNING -gt 100 ] && echo "⚠️  Muchas execuciones en progreso: $RUNNING"
```

---

## 7. Optimizaciones Comunes

### Aumentar memoria disponible
```bash
docker update --memory 4g n8n
docker restart n8n
```

### Variables de entorno importantes
```bash
# En .env o docker-compose.yml:
N8N_EXECUTION_TIMEOUT=3600000        # Timeout de ejecución (ms)
N8N_EXECUTION_DATA_PRUNE_INTERVAL=1  # Limpiar datos cada X horas
NODE_OPTIONS="--max-old-space-size=2048"  # Node.js heap
```

### Limpiar BD (si es PostgreSQL)
```bash
# Borrar ejecuciones antigas (> 30 días)
docker exec postgres psql -U n8n -d n8n -c "
DELETE FROM execution WHERE createdAt < NOW() - INTERVAL '30 days';"
```

---

## 8. Debugging

### Habilitar verbose logs
```bash
# En docker-compose.yml o .env:
DEBUG='n8n*'

# O al ejecutar:
docker run -e DEBUG=n8n* ... n8n
```

### Ver configuración actual
```bash
docker exec n8n env | grep -E "N8N|NODE"
```

### Conectar a base de datos (si PostgreSQL)
```bash
docker exec -it postgres psql -U n8n -d n8n
```

---

## 9. Backup y Recovery

### Backup de workflows
```bash
curl -s http://localhost:5678/api/v1/workflows | jq '.data' > workflows_backup.json
```

### Backup de credenciales (requiere acceso)
```bash
docker exec postgres pg_dump -U n8n -d n8n > n8n_backup.sql
```

### Restaurar desde backup
```bash
docker exec postgres psql -U n8n -d n8n < n8n_backup.sql
```

---

## 10. Conectar Credenciales

### Testear credencial
```bash
curl -s http://localhost:5678/api/v1/credentials/test \
  -H "X-N8N-AUTH: $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"credentials": {...}}'
```

### Listar credenciales
```bash
curl -H "X-N8N-AUTH: $TOKEN" http://localhost:5678/api/v1/credentials
```

---

## 11. WebSockets (Tiempo Real)

### Conexión WebSocket para notificaciones
```bash
# Útil para monitoring en vivo de workflows
ws://localhost:5679

# Cliente JS:
const ws = new WebSocket('ws://localhost:5679');
ws.onmessage = (event) => console.log(event.data);
```

---

## 12. Exportar e Importar Workflows

### Exportar un workflow
```bash
curl -s http://localhost:5678/api/v1/workflows/WORKFLOW_ID/download -o workflow.json
```

### Importar workflow
```bash
curl -X POST http://localhost:5678/api/v1/workflows \
  -F "file=@workflow.json"
```

---

## 13. Troubleshooting Rápido

| Problema | Solución |
|----------|----------|
| n8n no inicia | `docker logs n8n` - ver error |
| No conecta a BD | Verificar credenciales DB, connectivity |
| Workflows lentos | Revisar queries, CPU/RAM disponible |
| Ejecuciones atoradas | Reiniciar n8n, revisar logs |
| Sesión caducada | Token expiró, autenticarse nuevamente |
| API 401 Unauthorized | Token inválido o no enviado |

---

## 14. Herramientas Recomendadas

### n8n CLI
```bash
# Instalar (global)
npm install -g n8n

# Exportar workflows
n8n export:credentials --backup
n8n export:workflow --id 1 --backup
```

### Postman
- Importar colección de APIs de n8n
- Testear endpoints
- Documentar calls

### Monitor Docker
```bash
# Instalar Portainer para UI visual
docker run -d --name portainer \
  -p 8000:8000 -p 9000:9000 \
  -v /var/run/docker.sock:/var/run/docker.sock \
  portainer/portainer-ce
```

---

## 15. Documentación Específica

Para profundizar según tu setup:

- **Setup n8n-web**: [../n8n-web/doc/QUICK_START.md](../n8n-web/doc/QUICK_START.md)
- **Setup n8n-all-basico**: [../n8n-all-basico/doc/MONITOREO_RECURSOS.md](../n8n-all-basico/doc/MONITOREO_RECURSOS.md)
- **Setup distribuido**: [../n8n-workerflow-vps/doc/MONITOREO_RECURSOS.md](../n8n-workerflow-vps/doc/MONITOREO_RECURSOS.md)
- **Variables de entorno**: [../n8n-web/doc/VARIABLES_EXPLANATION.md](../n8n-web/doc/VARIABLES_EXPLANATION.md)

---

## 16. Cheatsheet Comprimido

```bash
# Health
curl http://localhost:5678/health

# Logs
docker logs n8n -f

# Stats
docker stats n8n

# Workflows activos
curl http://localhost:5678/api/v1/workflows | jq '.data | length'

# Ejecuciones
curl http://localhost:5678/api/v1/executions | jq '.data | length'

# Errores recientes
docker logs n8n | grep error | tail -5

# Reiniciar
docker restart n8n

# Config
docker exec n8n env | grep N8N
```

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0

**Índice de Documentación:**
- [MONITOREO_RECURSOS.md](./MONITOREO_RECURSOS.md) (este archivo)
- [n8n-web/doc/MONITOREO_RECURSOS.md](../n8n-web/doc/MONITOREO_RECURSOS.md)
- [n8n-all-basico/doc/MONITOREO_RECURSOS.md](../n8n-all-basico/doc/MONITOREO_RECURSOS.md)
- [n8n-workerflow-vps/doc/MONITOREO_RECURSOS.md](../n8n-workerflow-vps/doc/MONITOREO_RECURSOS.md)
