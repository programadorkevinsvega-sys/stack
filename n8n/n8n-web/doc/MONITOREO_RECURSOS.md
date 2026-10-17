# 📊 Monitoreo de Recursos - n8n (Web)

Documentación específica para monitorear el consumo de recursos del contenedor n8n (modo Web/UI).

---

## 1. Consumo General del Contenedor

### Ver stats en tiempo real
```bash
docker stats n8n --no-stream
```

### Monitoreo continuo
```bash
watch -n 5 'docker stats n8n --no-stream'
```

---

## 2. Consumo de MEMORIA

### Verificar límite de memoria
```bash
docker inspect n8n --format='{{.HostConfig.Memory}}' | awk '{print $1/1073741824 " GiB"}'
```

### Consumo actual
```bash
docker stats n8n --format "table {{.Container}}\t{{.MemUsage}}\t{{.MemPerc}}"
```

### Top procesos dentro del contenedor
```bash
docker exec n8n top -b -n 1 | head -15
```

### Node.js memory info
```bash
docker exec n8n node -e "console.log('Memory:', process.memoryUsage())"
```

---

## 3. Consumo de DISCO

### Tamaño del contenedor
```bash
docker ps -a --format "table {{.Names}}\t{{.Size}}" | grep n8n
```

### Tamaño del volumen
```bash
docker run --rm -v n8n:/home/node/.n8n busybox du -sh /home/node/.n8n
```

### Tamaño base de datos SQLite (local)
```bash
docker exec n8n ls -lh /home/node/.n8n/database.sqlite
```

### Logs del contenedor
```bash
docker exec n8n du -sh /home/node/.n8n/logs/
```

### Espacio disponible
```bash
docker exec n8n df -h /home/node
```

---

## 4. Consumo de CPU

### CPU actual
```bash
docker stats n8n --format "table {{.Container}}\t{{.CPUPerc}}"
```

### Procesos de CPU alto
```bash
docker exec n8n top -b -n 1 | sort -k9 -nr | head -10
```

---

## 5. Workflows y Ejecuciones

### Ver workflows activos (via API)
```bash
curl -s http://localhost:5678/api/v1/workflows \
  -H "X-N8N-AUTH: $N8N_AUTH_TOKEN" | jq '.[] | {id, name, active}'
```

### Información de un workflow
```bash
curl -s http://localhost:5678/api/v1/workflows/WORKFLOW_ID \
  -H "X-N8N-AUTH: $N8N_AUTH_TOKEN" | jq
```

### Ejecuciones recientes
```bash
curl -s "http://localhost:5678/api/v1/executions?limit=10" \
  -H "X-N8N-AUTH: $N8N_AUTH_TOKEN" | jq '.data[] | {id, workflowId, status, startedAt}'
```

### Ejecuciones en progreso
```bash
curl -s "http://localhost:5678/api/v1/executions?status=running" \
  -H "X-N8N-AUTH: $N8N_AUTH_TOKEN" | jq '.data | length'
```

---

## 6. Network (I/O)

### Tráfico de red
```bash
docker stats n8n --format "table {{.Container}}\t{{.NetIO}}"
```

### Conexiones activas
```bash
docker exec n8n netstat -tuln | grep -E "5678|5679"
```

---

## 7. Health y Performance

### Health check
```bash
curl -s http://localhost:5678/api/v1/nodes | jq '.statusCode'
```

### Status general
```bash
curl -s http://localhost:5678/health | jq
```

### Logs del contenedor
```bash
docker logs n8n -f --tail 100
```

### Buscar errores
```bash
docker logs n8n | grep -i error | tail -20
```

---

## 8. Base de Datos

### Si usa PostgreSQL (conectar y ver tamaño)
```bash
# Dentro de n8n (si está configurado con PostgreSQL)
docker exec postgres psql -U n8n -d n8n -c "
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as size
FROM pg_tables
WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;"
```

### Número de registros principales
```bash
docker exec postgres psql -U n8n -d n8n -c "
SELECT 
    'Workflows' as table_name, count(*) as rows FROM workflows
UNION ALL
SELECT 'Executions', count(*) FROM executions
UNION ALL
SELECT 'Credentials', count(*) FROM credentials_entity;"
```

---

## 9. Script de Monitoreo Completo

Crear archivo `monitor_n8n.sh`:

```bash
#!/bin/bash
# monitor_n8n.sh - Monitoreo de n8n

AUTH_TOKEN=${N8N_AUTH_TOKEN:-""}

echo "======================================"
echo "⚙️  MONITOREO N8N (WEB)"
echo "$(date)"
echo "======================================"
echo ""

echo "📊 STATS DEL CONTENEDOR"
docker stats n8n --no-stream
echo ""

echo "💾 CONSUMO DE DISCO"
echo "Tamaño del volumen:"
docker run --rm -v n8n:/data busybox du -sh /data 2>/dev/null || echo "N/A"
echo ""
if [ -f "/var/lib/docker/volumes/n8n/_data/database.sqlite" ]; then
  echo "Base de datos SQLite:"
  docker exec n8n ls -lh /home/node/.n8n/database.sqlite | awk '{print $5 " database.sqlite"}'
fi
echo ""

echo "🧠 MEMORIA (Node.js)"
docker exec n8n top -b -n 1 | grep -E "node|PID" | head -3
echo ""

echo "🔗 CONEXIONES"
docker exec n8n netstat -tuln | grep -E "5678|5679"
echo ""

echo "📈 WORKFLOWS Y EJECUCIONES"
if [ ! -z "$AUTH_TOKEN" ]; then
  echo "Total de workflows:"
  curl -s http://localhost:5678/api/v1/workflows \
    -H "X-N8N-AUTH: $AUTH_TOKEN" 2>/dev/null | jq '.data | length' || echo "Error"
  
  echo "Ejecuciones en progreso:"
  curl -s "http://localhost:5678/api/v1/executions?status=running" \
    -H "X-N8N-AUTH: $AUTH_TOKEN" 2>/dev/null | jq '.data | length' || echo "Error"
else
  echo "AUTH_TOKEN no configurado"
fi
echo ""

echo "✅ Fin del reporte - $(date)"
```

**Usar:**
```bash
chmod +x monitor_n8n.sh
export N8N_AUTH_TOKEN="your-token"
./monitor_n8n.sh
```

---

## 10. Puertos y Acceso

### Interfaces principales
- **UI Web:** http://localhost:5678
- **API:** http://localhost:5678/api/v1
- **WebSocket:** ws://localhost:5679

### Verificar que está respondiendo
```bash
curl -s http://localhost:5678/health | jq
```

---

## 11. Alertas Recomendadas

Monitorizar si:
- **CPU > 80%** constantemente
- **Memoria > 85%** del límite
- **Disk space < 20%**
- **Ejecuciones en progreso acumulándose** (sin completar)
- **Errores en logs** frecuentes
- **API no responde** (health check falla)

```bash
# Verificar salud
curl -s http://localhost:5678/health
```

---

## 12. Optimizaciones

### Aumentar memoria disponible
```bash
docker update --memory 4g n8n
docker restart n8n
```

### Variables de entorno para performance
```bash
# En .env agregar:
NODE_OPTIONS=--max-old-space-size=2048
N8N_EXECUTION_TIMEOUT=3600000
N8N_DEFAULT_BINARY_AD_FULLY_LOAD_CREDENTIALS=true
```

### Limpiar ejecuciones antiguas (si usa DB)
```bash
docker exec postgres psql -U n8n -d n8n -c "
DELETE FROM executions WHERE createdAt < NOW() - INTERVAL '30 days';"
```

---

## 13. Logs Importantes

### Inicialización
```bash
docker logs n8n | grep -E "Listen on|Worker|Loading|Registered" | head -20
```

### Credenciales (verificar carga)
```bash
docker logs n8n | grep -i credential
```

### Workers conectados
```bash
docker logs n8n | grep -i "worker\|connected"
```

---

## 14. Debugging

### Habilitar verbose logging
```bash
# En docker-compose.yml:
# environment:
#   DEBUG: 'n8n*'

docker logs n8n -f --tail 50
```

### Ver configuración actual
```bash
docker exec n8n env | grep N8N
```

---

## Cheatsheet Rápido

| Comando | Propósito |
|---------|-----------|
| `docker stats n8n` | Stats en tiempo real |
| `docker logs n8n -f` | Logs en vivo |
| `curl http://localhost:5678/health` | Health check |
| `curl http://localhost:5678/api/v1/workflows` | Listar workflows |
| `docker restart n8n` | Reiniciar |
| `df -h /var/lib/docker/volumes` | Espacio volúmenes |

---

**Última actualización:** 2026-04-03  
**Versión:** 1.0
