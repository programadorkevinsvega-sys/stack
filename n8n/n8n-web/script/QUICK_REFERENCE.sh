#!/usr/bin/env bash

# 🎯 QUICK REFERENCE - 30 Segundo Cheat Sheet
# ════════════════════════════════════════════════════════

cat << 'EOF'

╔════════════════════════════════════════════════════════════════════════════╗
║                    N8N DISTRIBUTED DEPLOYMENT                             ║
║                                                                            ║
║  "¿Dónde corre qué?" - La versión más simple posible                     ║
╚════════════════════════════════════════════════════════════════════════════╝

┏─────────────────────────────────────────────────────────────────────────────┓
┃ 📦 SERVICIOS COMPARTIDOS (SERVIDOR EXTERNO - YA DEBEN EXISTIR)            ┃
├─────────────────────────────────────────────────────────────────────────────┤
┃                                                                             ┃
┃  Redis:8:6379           → Cola de trabajos                                 ┃
┃  PostgreSQL:5432        → Base de datos                                    ┃
┃  Vault:8200             → Secretos (opcional)                              ┃
┃                                                                             ┃
┃  🔗 Accesible desde: PC LOCAL + VPS                                        ┃
┃  ⚠️  NO están en docker-compose.yml de este repo                          ┃
┃                                                                             ┃
└─────────────────────────────────────────────────────────────────────────────┘


┏─────────────────────────────────────────────────────────────────────────────┓
┃ 🖥️  PC LOCAL (Tu computadora)                                               ┃
├─────────────────────────────────────────────────────────────────────────────┤
┃                                                                             ┃
┃  ┌──────────────────────────────────────────────────────────────────────┐  ┃
┃  │ Docker Compose: docker-compose.yml                                   │  ┃
┃  │                                                                      │  ┃
┃  │  ✓ n8n-web          (puerto 5678) - INTERFAZ GRÁFICA              │  ┃
┃  │  ✓ n8n-worker       (2 instancias) - PROCESAMIENTO                │  ┃
┃  │                                                                      │  ┃
┃  │  ❌ Redis           (en servidor EXTERNO)                          │  ┃
┃  │  ❌ PostgreSQL      (en servidor EXTERNO)                          │  ┃
┃  │  ❌ Vault           (en servidor EXTERNO)                          │  ┃
┃  └──────────────────────────────────────────────────────────────────────┘  ┃
┃                                                                             ┃
┃  Scripts que ejecutas aquí:                                                ┃
┃  - ./verify-config.sh           (verifica antes de iniciar)               ┃
┃  - docker compose up -d         (levanta n8n)                             ┃
┃  - ./backup.sh                  (backup de BD)                            ┃
┃  - ./vps-helper.sh scale 5      (escala VPS remotamente)                 ┃
┃                                                                             ┃
│  Variables: .env (copia de .env.example)                                  ┃
│  ├─ N8N_ENCRYPTION_KEY (MUST MATCH con VPS)                              ┃
│  ├─ QUEUE_BULL_REDIS_HOST=192.168.1.50                                   ┃
│  └─ DB_POSTGRESDB_HOST=192.168.1.50                                      ┃
┃                                                                             ┃
└─────────────────────────────────────────────────────────────────────────────┘


┏─────────────────────────────────────────────────────────────────────────────┓
┃ 🚀 VPS REMOTO (Servidor en la nube)                                         ┃
├─────────────────────────────────────────────────────────────────────────────┤
┃                                                                             ┃
┃  ┌──────────────────────────────────────────────────────────────────────┐  ┃
┃  │ Docker Compose: docker-compose.vps.yml                                │  ┃
┃  │                                                                      │  ┃
┃  │  ✓ n8n-worker       (3+ instancias) - PROCESAMIENTO REMOTO         │  ┃
┃  │                                                                      │  ┃
┃  │  ❌ n8n-web         (en PC LOCAL)                                   │  ┃
┃  │  ❌ Redis           (en servidor EXTERNO)                          │  ┃
┃  │  ❌ PostgreSQL      (en servidor EXTERNO)                          │  ┃
┃  │  ❌ Vault           (en servidor EXTERNO)                          │  ┃
┃  └──────────────────────────────────────────────────────────────────────┘  ┃
┃                                                                             ┃
┃  Quien lo levanta: TÚ vía SSH o ./vps-helper.sh desde PC                 ┃
┃                                                                             ┃
│  Variables: .env (mismas que .env.example.vps)                            ┃
│  ├─ N8N_ENCRYPTION_KEY (MUST MATCH con PC)                               ┃
│  ├─ QUEUE_BULL_REDIS_HOST=192.168.1.50 (SAME)                           ┃
│  └─ DB_POSTGRESDB_HOST=192.168.1.50 (SAME)                              ┃
┃                                                                             ┃
└─────────────────────────────────────────────────────────────────────────────┘


╔════════════════════════════════════════════════════════════════════════════╗
║                        FLUJO DE DATOS SIMPLE                               ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  1. Abres: http://localhost:5678                  ← PC LOCAL n8n-web     ║
║  2. Creas un workflow                                                     ║
║  3. Presionas "Save & Execute"                                            ║
║  4. n8n envía el trabajo a Redis                  ← SERVIDOR EXTERNO     ║
║  5. Workers (PC + VPS) escuchan en Redis                                  ║
║  6. Alguien lo procesa (PC worker O VPS worker)                           ║
║  7. Resultado guardado en PostgreSQL              ← SERVIDOR EXTERNO     ║
║  8. Verificas resultado en http://localhost:5678  ← PC LOCAL n8n-web     ║
║                                                                            ║
╚════════════════════════════════════════════════════════════════════════════╝


╔════════════════════════════════════════════════════════════════════════════╗
║                           CHECKLIST INICIAL                                ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  ☐ ¿Redis accesible?                                                      ║
║    redis-cli -h 192.168.1.50 ping                                          ║
║    Respuesta esperada: PONG                                               ║
║                                                                            ║
║  ☐ ¿PostgreSQL accesible?                                                 ║
║    psql -h 192.168.1.50 -U n8n_user -d n8n -c "SELECT 1"                 ║
║    Respuesta esperada: 1                                                  ║
║                                                                            ║
║  ☐ ¿Variables correctas?                                                  ║
║    ./verify-config.sh                                                     ║
║    Respuesta esperada: ✅ All checks passed                               ║
║                                                                            ║
║  ☐ ¿Docker funcionando?                                                   ║
║    docker ps                                                              ║
║    docker compose --version                                               ║
║                                                                            ║
║  ➜ ENTONCES levanta:                                                      ║
║    docker compose up -d                                                   ║
║                                                                            ║
╚════════════════════════════════════════════════════════════════════════════╝


╔════════════════════════════════════════════════════════════════════════════╗
║                      COMANDOS MÁS USADOS                                   ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  PC LOCAL                                                                  ║
║  ─────────────────────────────────────────────────────────────────────    ║
║  docker compose up -d              Levanta n8n local                      ║
║  docker compose down               Apaga n8n local                        ║
║  docker compose logs -f            Ve logs en tiempo real                 ║
║  ./verify-config.sh                Verifica configuración                 ║
║  ./backup.sh                       Hace backup de BD                      ║
║                                                                            ║
║  VPS REMOTO (desde PC)                                                    ║
║  ─────────────────────────────────────────────────────────────────────    ║
║  ./vps-helper.sh status            Ver estado de workers VPS              ║
║  ./vps-helper.sh scale 5           Escalar a 5 workers                    ║
║  ./vps-helper.sh logs-workers      Ver logs de workers VPS                ║
║  ./vps-helper.sh restart-workers   Reiniciar workers VPS                  ║
║  ./vps-helper.sh queue-length      Ver trabajos en cola                   ║
║                                                                            ║
║  DIRECTO EN VPS (vía SSH)                                                 ║
║  ─────────────────────────────────────────────────────────────────────    ║
║  docker compose -f docker-compose.vps.yml up -d                           ║
║  docker compose -f docker-compose.vps.yml down                            ║
║  docker compose -f docker-compose.vps.yml logs -f                         ║
║                                                                            ║
╚════════════════════════════════════════════════════════════════════════════╝


╔════════════════════════════════════════════════════════════════════════════╗
║                          ESTO NO DEBE HACER                                ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  ❌ NO ejecutar docker compose para Redis/PostgreSQL aquí                 ║
║     (Ya existen en otro servidor)                                          ║
║                                                                            ║
║  ❌ NO copiar elementos de "SERVICIOS EXTERNOS" a docker-compose         ║
║     (Causara duplicados e conflictos)                                     ║
║                                                                            ║
║  ❌ NO usar IPs diferentes para REDIS_HOST en PC vs VPS                   ║
║     (Deben conectarse a MISMO Redis)                                      ║
║                                                                            ║
║  ❌ NO usar IPs diferentes para DB_HOST en PC vs VPS                      ║
║     (Deben conectarse a MISMO PostgreSQL)                                 ║
║                                                                            ║
║  ❌ NO usar diferentes N8N_ENCRYPTION_KEY en PC vs VPS                    ║
║     (Causará error al compartir credenciales)                             ║
║                                                                            ║
║  ❌ NO ejecutar backup.sh en ambos (PC + VPS)                             ║
║     (Hará backups duplicados)                                             ║
║                                                                            ║
╚════════════════════════════════════════════════════════════════════════════╝


╔════════════════════════════════════════════════════════════════════════════╗
║                         PARA MÁS DETALLES VER                              ║
╠════════════════════════════════════════════════════════════════════════════╣
║                                                                            ║
║  📖 ARCHITECTURE_SIMPLE.md      Explicación en profundidad                 ║
║  📖 SCRIPT_LOCATION.md          Dónde ejecutar cada script                 ║
║  📖 VARIABLES_EXPLANATION.md    Qué significa cada variable                ║
║  📖 QUICK_START.md              Inicio en 5 minutos                        ║
║  📖 README.md                   Guía principal completa                    ║
║                                                                            ║
╚════════════════════════════════════════════════════════════════════════════╝

EOF
