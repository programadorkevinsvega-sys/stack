# 🔐 Vault - Guía Operativa del Stack

**Ubicación:** `database/vault/doc/README.md`  
**Propósito:** Comandos exactos para inicializar, abrir y administrar el Vault de este repo  
**Última actualización:** 2026-04-13

---

## 📌 Antes de empezar

Este stack de Vault usa:

- **HTTPS obligatorio**
- **Puerto real:** `8500`
- **API/CLI local confiable:** `https://127.0.0.1:8500`
- **UI local:** `https://localhost:8500/ui`
- **CA local:** `database/vault/ssl/generales/root.crt`
- **Datos persistentes:** `database/vault/volumes/data`
- **Config:** `database/vault/vault.hcl`

No uses estos valores en este repo porque te van a fallar:

- `http://localhost:8500`
- `https://0.0.0.0:8500`
- cualquier ejemplo con puerto `8200`

El certificado actual solo tiene SAN para `127.0.0.1`, `localhost` y `100.121.34.76`.  
Si otro contenedor quiere conectarse por TLS usando el hostname `vault`, primero hay que regenerar el certificado incluyendo ese DNS.

---

## 🔑 Qué clave es cuál

| Dato | Para qué sirve | Cómo se obtiene |
|------|----------------|-----------------|
| **Unseal Key** | Abrir Vault cuando está `sealed` | Sale una sola vez en `vault operator init` |
| **Initial Root Token** | Login administrativo inicial | Sale una sola vez en `vault operator init` |
| **RoleID** | Identificador del AppRole de un servicio | `vault read auth/approle/role/<rol>/role-id` |
| **SecretID** | Secreto del AppRole | `vault write -f auth/approle/role/<rol>/secret-id` |
| **UserPass** | Usuario humano para entrar por CLI/UI | `vault write auth/userpass/users/<usuario> ...` |

Punto importante:

- `Unseal Key` y `Initial Root Token` no se “recrean” con un comando mágico sobre una instancia ya inicializada.
- `RoleID`, `SecretID`, usuarios y passwords sí se pueden volver a generar o rotar.

---

## 🧰 Helper recomendado

Ejecuta esto una vez en tu shell desde la raíz del repo:

```bash
vault_local() {
  docker exec \
    -e VAULT_ADDR=https://127.0.0.1:8500 \
    -e VAULT_CACERT=/vault/config/tls/root.crt \
    vault vault "$@"
}
```

Con eso, todos los comandos siguientes se vuelven más cortos.

---

## 🚀 Estado actual de Vault

### 1. Levantar el contenedor
```bash
docker compose -f database/vault/docker-compose.vault.yml up -d
```

### 2. Ver si Vault ya fue inicializado
```bash
curl -sS --cacert database/vault/ssl/generales/root.crt \
  https://127.0.0.1:8500/v1/sys/init | jq
```

### 3. Ver estado general
```bash
vault_local status
```

Interpretación rápida:

- `Initialized: false` -> todavía no existen `Unseal Key` ni `Root Token`
- `Initialized: true` + `Sealed: true` -> ya existe Vault, pero necesitas la `Unseal Key` guardada
- `Initialized: true` + `Sealed: false` -> ya puedes hacer login y administrar secretos

---

## 🆕 Si Vault NO está inicializado

### 1. Inicializar y generar claves nuevas
```bash
vault_local operator init -key-shares=1 -key-threshold=1
```

Ese comando imprime exactamente una vez:

- `Unseal Key 1: ...`
- `Initial Root Token: ...`

Guárdalos fuera del repo.

### 2. Abrir Vault
```bash
vault_local operator unseal <UNSEAL_KEY>
```

### 3. Login administrativo
```bash
vault_local login <INITIAL_ROOT_TOKEN>
```

---

## 🔒 Si Vault YA está inicializado

Esto es lo que pasa en una instancia ya creada:

- `vault_local operator init ...` ya no genera nuevas claves
- si está `sealed`, necesitas la `Unseal Key` que se guardó cuando se hizo el `init`
- si perdiste la `Unseal Key` y el `Root Token` en este entorno de desarrollo, la salida práctica suele ser resetear datos y volver a inicializar

### Abrir una instancia ya existente
```bash
vault_local operator unseal <UNSEAL_KEY_EXISTENTE>
```

### Login con token admin ya existente
```bash
vault_local login <ROOT_TOKEN_EXISTENTE>
```

---

## 💥 Reset total y creación de claves nuevas

Esto borra todos los secretos guardados en esta instancia local.

```bash
docker compose -f database/vault/docker-compose.vault.yml down
rm -rf database/vault/volumes/data/*
docker compose -f database/vault/docker-compose.vault.yml up -d
vault_local operator init -key-shares=1 -key-threshold=1
```

Después del `init`, sigue con:

```bash
vault_local operator unseal <UNSEAL_KEY_NUEVA>
vault_local login <INITIAL_ROOT_TOKEN_NUEVO>
```

---

## 🛠️ Operaciones básicas

### Crear o actualizar un secreto
```bash
vault_local kv put secret/myapp/database \
  username="user123" \
  password="secret_pass" \
  host="db.example.com"
```

### Leer un secreto
```bash
vault_local kv get secret/myapp/database
```

### Listar secretos
```bash
vault_local kv list secret/
```

### Eliminar un secreto
```bash
vault_local kv delete secret/myapp/database
```

---

## 🔑 Crear un AppRole exacto para un servicio

### 1. Habilitar AppRole
```bash
vault_local auth enable approle
```

Si responde que ya existe, sigue al siguiente paso.

### 2. Verificar que el motor `secret/` exista
```bash
vault_local secrets list
```

Si `secret/` no aparece, créalo:

```bash
vault_local secrets enable -path=secret kv-v2
```

### 3. Crear política para `n8n`
```bash
cat <<'EOF' | docker exec -i \
  -e VAULT_ADDR=https://127.0.0.1:8500 \
  -e VAULT_CACERT=/vault/config/tls/root.crt \
  vault vault policy write n8n-policy -
path "secret/data/n8n/*" {
  capabilities = ["read"]
}

path "secret/metadata/n8n/*" {
  capabilities = ["list"]
}
EOF
```

### 4. Crear el rol
```bash
vault_local write auth/approle/role/n8n-role \
  token_policies="n8n-policy" \
  token_ttl=24h \
  token_max_ttl=24h
```

### 5. Obtener el `RoleID`
```bash
vault_local read auth/approle/role/n8n-role/role-id
```

### 6. Generar un `SecretID` nuevo
```bash
vault_local write -f auth/approle/role/n8n-role/secret-id
```

Lo que se regenera normalmente cuando un servicio falla por credenciales es el `SecretID`, no la `Unseal Key`.

---

## 👤 Crear o rotar usuario/password (UserPass)

### 1. Habilitar UserPass
```bash
vault_local auth enable userpass
```

### 2. Crear usuario
```bash
vault_local write auth/userpass/users/miusuario \
  password="mi_password_seguro" \
  policies="n8n-policy"
```

### 3. Cambiar contraseña
```bash
vault_local write auth/userpass/users/miusuario \
  password="mi_password_nuevo" \
  policies="n8n-policy"
```

### 4. Login con UserPass
```bash
vault_local login -method=userpass username=miusuario password=mi_password_nuevo
```

---

## 📦 Secreto exacto para n8n

### Guardar credenciales de base de datos
```bash
vault_local kv put secret/n8n/prod_db \
  username="usuario_bd" \
  password="password_bd" \
  database_host="postgres" \
  database_port="5432" \
  database_name="n8n"
```

### Leerlas
```bash
vault_local kv get secret/n8n/prod_db
```

### Listar paths bajo `n8n`
```bash
vault_local kv list secret/n8n/
```

Nota:

- en CLI se usa `secret/n8n/prod_db`
- en API KV v2 el path equivalente es `secret/data/n8n/prod_db`

---

## 🚨 Troubleshooting real de este repo

### Error TLS con `0.0.0.0`

Síntoma típico:

```text
x509: certificate is valid for 100.121.34.76, 127.0.0.1, not 0.0.0.0
```

Solución:

- usa `https://127.0.0.1:8500`
- usa `VAULT_CACERT=/vault/config/tls/root.crt` dentro del contenedor
- usa `--cacert database/vault/ssl/generales/root.crt` desde el host

### Vault no responde
```bash
docker logs vault --tail 100
```

### Vault sigue sellado
```bash
vault_local status
vault_local operator unseal <UNSEAL_KEY>
```

### Permiso denegado al leer un secreto
```bash
vault_local policy list
vault_local policy read n8n-policy
```

### No sabes si el auth method existe
```bash
vault_local auth list
```

---

## 🔗 URLs correctas

- **API local:** `https://127.0.0.1:8500`
- **UI local:** `https://localhost:8500/ui`
- **Tailscale:** `https://100.121.34.76:8500/ui`

---

## 📚 Documentación relacionada

- **Monitoreo y diagnóstico:** `database/vault/doc/MONITOREO_RECURSOS.md`
- **Guía específica para n8n:** `database/vault/.archive/n8n.md`
- **TLS y certificados:** `database/vault/ssl/README.md`

---

**Archivo:** `database/vault/doc/README.md`  
**Versión:** 2.0  
**Estado:** Documentación corregida para este stack
