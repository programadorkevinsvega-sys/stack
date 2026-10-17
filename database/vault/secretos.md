# Vault - Secretos de acceso

> Generado: 2026-05-10
> Vault versión: 1.15.0
> Contenedor: vault (puerto 8500)
> Método: Shamir (5 shares, threshold 3)

## Unseal Keys

| # | Key |
|---|-----|
| 1 | `W9MFI2P7ivvwFyzHfZTa0YpbhXVKQ/KP+oeJYCT+o5jB` |
| 2 | `DBVDxm86uUvMVwZZIVJ+rY0OVy5iEI3jJ62UlAOoXyzM` |
| 3 | `0pQscuPVa6yiHGpNdePUz1vr+yk6guLLqqJ8sTfloCer` |
| 4 | `I/B6clkA5vnt11TqaG/Spdi9B98a6aq8iNQqpgBJ6K7z` |
| 5 | `YYXRxPjsSs/xy3KQFCHsIktjDEf630GI1JZYIlxXNXbo` |

## Root Token

```
hvs.N059OLdwSEQE57837MCqTujc
```

## Comandos rápidos

```bash
# Variables de entorno
export VAULT_ADDR="http://127.0.0.1:8500"

# Estado
vault status

# Login con root token
vault login hvs.N059OLdwSEQE57837MCqTujc

# Listar secretos
vault secrets list
```

## Scripts (en orden)

| # | Script | Qué hace |
|---|--------|----------|
| 1 | `script/01-start.sh` | Construye imagen e inicia contenedor |
| 2 | `script/02-init.sh` | Inicializa Vault (genera keys + root token) |
| 3 | `script/03-unseal.sh` | Desella Vault (usa 3 de 5 keys) |
| 4 | `script/04-login.sh` | Login con root token |
| 5 | `script/05-status.sh` | Ver estado completo |

> ⚠️ Este archivo contiene credenciales sensibles. No lo compartas ni lo subas a git.
