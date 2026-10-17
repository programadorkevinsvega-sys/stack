# Plan de migración de Vault a servicios administrados de AWS

> Sustitución de HashiCorp Vault KV/PKI por AWS Systems Manager Parameter
> Store, AWS Secrets Manager y AWS Private CA/ACM. La ruta AWS ya está activa
> en desarrollo; las tareas pendientes son gates de operación productiva.

## Estado de implementación (2026-07-25)

- **Código:** `AppConfig::from_env_async()` carga Secrets Manager y SSM; un
  error de AWS es fatal y no existe fallback automático a Vault.
- **`.env`:** `SECRETS_PROVIDER=aws` y `TLS_CERT_PROVIDER=aws`; cada secreto
  migrado permanece comentado con su referencia AWS.
- **`.env.example`:** conserva valores de referencia/placeholders y marca en
  la misma línea si el valor real proviene de SSM o Secrets Manager.
- **Scripts:** `run-all.sh` y verificadores no ejecutan ni leen el runtime de
  Vault; las migraciones consultan Secrets Manager directamente.
- **Compose:** Vault quedó en el perfil opcional `legacy-vault`; no arranca en
  el perfil por defecto y no contiene el seed de credenciales.
- **AWS dev:** secreto `ai-gateway/dev/runtime`, CMK
  `alias/ai-gateway-secrets`, perfil local restringido y CA subordinada ACM
  Private CA con bundles TLS por servicio.
- El usuario IAM de ejecución tiene `ssm:PutParameter` limitado únicamente a
  `arn:aws:ssm:us-east-1:058009633785:parameter/ai-gateway/dev/common/*`; no
  puede escribir parámetros de otros servicios ni fuera del entorno `dev`.
- **Pendiente para producción:** secretos separados por servicio, parámetros
  SSM completos, roles IAM/OIDC por servicio, CloudTrail/alertas, rotación
  automática y retirada física de Vault tras los gates E2E.

## Objetivo y límites

La fuente de configuración y secretos quedará separada por responsabilidad:

| Tipo | Servicio AWS | Ejemplos |
|---|---|---|
| Configuración técnica no sensible | SSM Parameter Store | timeouts, pools, límites técnicos, endpoints, modelo activo y flags de infraestructura |
| Secretos críticos | AWS Secrets Manager | contraseñas, URLs con credenciales, API keys, JWT, PayPal, R2, Qdrant y OpenRouter |
| Cifrado | AWS KMS | claves de cifrado de MFA y CMK de Secrets Manager |
| Certificados privados y CA | AWS Private CA + ACM | CA interna, certificados de servicio y cadena de confianza mTLS |
| Configuración de negocio | PostgreSQL mediante Admin BFF | planes, precios, cuotas, límites comerciales y capacidades por usuario |

La migración no mueve la configuración de planes al Parameter Store. El Admin
Dashboard seguirá escribiendo reglas de negocio en PostgreSQL, con auditoría e
invalidación de caché.

## Reglas de seguridad no negociables

1. Ningún secreto, certificado, clave privada o credencial se guarda en Git,
   `.env`, frontend, imagen de servicio, logs o parámetros `String`.
2. Los microservicios no usarán credenciales raíz de AWS.
3. En producción se usará IAM Role, IRSA/OIDC o la identidad administrada de la
   plataforma. En desarrollo local se usará un perfil AWS de la CLI.
4. Las credenciales estáticas de AWS solo se almacenarán en Secrets Manager si
   una integración heredada las exige y siempre con permisos mínimos. No se
   usarán para resolver el bootstrap de la propia identidad AWS.
5. Secrets Manager y ACM/Private CA usarán CMK KMS dedicada, con CloudTrail y
   rotación configurada.
6. Cada servicio tendrá una identidad IAM distinta y solo podrá leer sus
   parámetros y secretos necesarios.
7. Los cambios de configuración y secretos tendrán versión, actor, motivo,
   fecha, evidencia de prueba y rollback documentado.

## Topología y nombres canónicos

La topología debe ser explícita por cuenta y entorno:

```text
/ai-gateway/{environment}/{service}/{PARAMETER_NAME}
ai-gateway/{environment}/{service}/runtime
ai-gateway/{environment}/tls/{service}
```

En el entorno dev actual, todos los binarios cargan el secreto común
`ai-gateway/dev/runtime` (o el valor explícito de `AWS_RUNTIME_SECRET_ID`) y
después incorporan sus parámetros SSM bajo `/ai-gateway/dev/{service}`. La
separación en secretos `runtime` por servicio queda reservada para el
despliegue productivo con IAM de mínimo privilegio.

Ejemplos:

```text
/ai-gateway/prod/common/OTEL_EXPORTER_OTLP_ENDPOINT
/ai-gateway/prod/file-service/EMBEDDING_MAX_BATCH_TOKENS
ai-gateway/prod/auth-service/runtime
ai-gateway/prod/tls/auth-service
```

Cada secreto `runtime` será un JSON versionado y atómico:

```json
{
  "AUTH_SERVICE_DATABASE_URL": "postgres://...",
  "JWT_SIGNING_SECRET": "...",
  "AUTH_MFA_KMS_KEY_ID": "alias/ai-gateway-auth-mfa"
}
```

El bundle TLS tendrá una estructura separada:

```json
{
  "certificate_chain_pem": "-----BEGIN CERTIFICATE-----\\n...",
  "private_key_pem": "-----BEGIN PRIVATE KEY-----\\n...",
  "ca_bundle_pem": "-----BEGIN CERTIFICATE-----\\n...",
  "serial_number": "...",
  "not_after": "2026-08-01T00:00:00Z",
  "version": "..."
}
```

## Fase 0 — Inventario y congelamiento de la configuración

1. Enumerar todas las variables actuales de `.env`, `.env.example`, Vault KV,
   `vault/runtime-secret-keys.txt`, `docker-compose.yml` y los servicios Rust.
2. Clasificar cada variable como parámetro técnico, secreto, certificado,
   configuración de negocio o valor local de desarrollo.
3. Identificar duplicados y defaults peligrosos. No se migrarán defaults que
   oculten un secreto o un bucket equivocado.
4. Registrar dependencias de cada servicio y permisos mínimos requeridos.
5. Crear el inventario firmado en `artifacts/aws-migration/inventory.json`.

**Gate:** cero variables críticas sin clasificación y cero secretos nuevos en
`.env`.

## Fase 1 — Fundación de AWS e IAM

1. Confirmar cuenta, región y separación de entornos (`dev`, `staging`, `prod`).
2. Crear CMK KMS para Secrets Manager y otra clave para cargas que requieran
   separación criptográfica, con alias estables y políticas sin acceso raíz
   operativo.
3. Crear roles IAM por servicio: `api-gateway`, `auth-service`,
   `file-service`, `admin-bff`, `s3-gateway`, etc.
4. Conceder únicamente:
   - `ssm:GetParameter`/`ssm:GetParametersByPath` sobre el prefijo del servicio.
   - `secretsmanager:GetSecretValue` sobre sus secretos.
   - `kms:Decrypt` sobre la CMK correspondiente.
   - `acm-pca:IssueCertificate` y `acm-pca:GetCertificate` solo al emisor de
     certificados, nunca a todos los microservicios.
5. Activar CloudTrail para SSM, Secrets Manager, KMS, ACM y Private CA.
6. Configurar alertas de lectura anómala, denegaciones IAM, rotación vencida y
   uso desde una región no autorizada.

**Gate:** cada rol puede leer únicamente sus propios parámetros y secretos;
una prueba negativa de IAM debe devolver `AccessDenied`.

## Fase 2 — Migración de configuración a Parameter Store (parcial)

Mover solo valores no sensibles:

- timeouts y límites técnicos;
- tamaños de pool y concurrencia;
- nombres y versiones de modelos;
- endpoints internos sin credenciales;
- configuración OTEL y observabilidad;
- flags operativos de infraestructura;
- configuración específica por entorno.

Los nombres de planes, precios, cuotas y reglas comerciales permanecen en
PostgreSQL y no se duplican en SSM.

La aplicación soporta `AWS_PARAMETER_PREFIX` y carga parámetros con una llamada
controlada, validando tipo, rango y versión. No existe lectura dual hacia Vault;
producción debe fallar si falta un parámetro obligatorio. El aprovisionamiento
completo de parámetros SSM queda pendiente del gate productivo.

**Gate:** reporte de paridad entre el valor anterior y SSM, sin parámetros
secretos almacenados como `String`.

## Fase 3 — Migración de secretos a Secrets Manager (dev completado)

1. En dev existe el secreto atómico `ai-gateway/dev/runtime`. Antes de
   producción debe dividirse en secretos `runtime` por servicio para cumplir
   mínimo privilegio.
2. Migrar contraseñas y URLs autenticadas de PostgreSQL, Redis y NATS.
3. Migrar credenciales de R2, Qdrant, OpenRouter, Resend y PayPal.
4. Migrar peppers, secretos JWT, `AUDIT_HMAC_SECRET` y
   `OBSERVABILITY_USER_HASH_SALT`.
5. Mantener `AUTH_MFA_KMS_KEY_ID` como configuración no secreta, salvo que una
   política interna exija incluirlo en el secreto del servicio.
6. Configurar versiones `AWSCURRENT` y `AWSPREVIOUS` para permitir rollback.
7. Implementar rotación coordinada para PostgreSQL, Redis, NATS, PayPal y
   proveedores externos. Una rotación de contraseña de PostgreSQL debe cambiar
   primero el servidor, validar la nueva credencial y solo después promover la
   nueva versión.
8. Prohibir que el frontend o Admin BFF lean secretos; el Admin Dashboard solo
   administra referencias y configuración autorizada.

**Gate:** lectura exitosa desde cada servicio con su rol IAM, sin valor secreto
en logs ni en variables del repositorio.

## Fase 4 — Identidad de ejecución y bootstrap (dev completado)

1. Eliminar la dependencia de credenciales raíz o access keys raíz.
2. Desarrollo local: perfil `ai-gateway-auth-service` de la CLI con permisos
   limitados a lectura del runtime/SSM y descifrado KMS.
3. Producción: IAM Role/IRSA/OIDC o identidad administrada según la plataforma.
4. El proceso obtiene primero una identidad AWS; con ella lee SSM y Secrets
   Manager. Nunca se intenta obtener la identidad AWS desde el mismo secreto que
   esa identidad necesita leer.
5. Implementar caché en memoria con TTL corto, invalidación por versión y
   comportamiento fail-closed para secretos obligatorios.
6. Registrar únicamente `secret_id`, versión, servicio y resultado; nunca el
   contenido.

**Gate:** el servicio arranca con perfil/rol IAM y falla de forma explícita con
   permisos insuficientes; no depende de `.runtime/vault`.

## Fase 5 — PKI con AWS Private CA y ACM (emisión dev completada)

1. Se creó una raíz externa protegida y una CA subordinada operativa en AWS
   Private CA; los certificados dev se almacenan por servicio en Secrets
   Manager.
2. Definir una plantilla de emisión para `serverAuth` y `clientAuth`, con SAN
   restringido al servicio y a los nombres internos permitidos.
3. Crear un emisor central con permisos de `IssueCertificate` y
   `GetCertificate`. Los servicios consumidores solo leen su bundle asignado.
4. Generar una clave privada por servicio y una CSR. La clave privada se guarda
   en Secrets Manager; Private CA solo firma la CSR.
5. Guardar el certificado, la cadena y la CA en el secreto TLS del servicio.
6. Configurar TTL corto, renovación anticipada, métricas de expiración y
   alarmas de certificados sin renovación.
7. Implementar renovación automática: emitir nuevo certificado, publicar una
   nueva versión del secreto, validar el bundle y recargarlo antes de promover
   la versión.
8. Para listeners gRPC que no soporten recarga hot, usar reinicio controlado y
   escalonado antes de `TLS_MIN_VALIDITY_SECONDS`; no esperar a la expiración.

**Gate:** prueba real de conexión mTLS con certificado nuevo, certificado
revocado y certificado expirado; ningún servicio acepta un SAN de otro
servicio.

## Fase 6 — Adaptación de `crates/common` (carga AWS completada)

1. Añadir proveedores AWS separados:
   - `ParameterStoreConfigProvider` para parámetros.
   - `SecretsManagerProvider` para secretos y bundles TLS.
   - `PrivateCaCertificateIssuer` en el proceso de emisión, no en cada servicio.
2. Mantener contratos de configuración independientes del SDK de AWS mediante
   traits internos y adaptadores de infraestructura.
3. `AppConfig::from_env_async()` carga Secrets Manager y SSM; si AWS falla la
   configuración falla cerrada. La ruta Vault solo existe como proveedor legado
   explícito y nunca se selecciona automáticamente.
4. Sustituir `VaultCertificateProvider` por lectura del bundle TLS desde
   Secrets Manager y validación X.509.
5. Implementar refresco por versión/TTL y métricas de carga, errores,
   expiración, latencia y versión activa.
6. Invalidar y reconstruir clientes gRPC cuando se renueve material TLS. No
   afirmar que un `ServerTlsConfig` estático se renovó solo por refrescar la
   caché.
7. Mantener `TLS_MODE=disabled|tls|mtls`, pero cambiar el proveedor AWS como
   único proveedor soportado en producción.

**Gate:** `cargo check --workspace`, pruebas de provider, pruebas de
certificado inválido, secreto ausente, `AccessDenied`, timeout y versión
anterior.

## Fase 7 — Migración de los microservicios y scripts (AWS completada)

Actualizar todos los servicios para cargar AWS al inicio y durante la
renovación:

- `api-gateway`, `api-service`, `auth-service`, `credential-manager`,
  `key-services`, `model-router`.
- `subscription-service`, `payment-services`, `billing-service`,
  `usage-service`, `invoice-service`.
- `customer-bff`, `admin-bff`, `file-service` y `s3-gateway`.

Actualizar:

- `.env.example` con referencias AWS no sensibles y sin secretos reales.
- `.env` local sin contraseñas ni API keys.
- `run-all.sh` para esperar AWS/SSM/Secrets Manager y no exportar secretos.
- `build-all.sh` para validar que no empaqueta material secreto.
- pruebas de arranque y health checks.

El `run-all.sh` no ejecuta Vault, no lee `runtime.env` de Vault ni copia
secretos a procesos. Los servicios obtienen configuración mediante AWS.

## Fase 8 — Eliminación controlada de Vault (sin fallback)

1. No existe período dual ni fallback a Vault. El rollback de configuración se
   realiza únicamente con versiones `AWSCURRENT`/`AWSPREVIOUS` de AWS.
2. Comparar hashes de nombres, versiones y expiración, nunca valores en logs.
3. Cambiar producción a AWS únicamente después de completar todos los gates.
4. Detener `vault-bootstrap` y retirar Vault del `docker-compose.yml` después
   de completar el gate E2E del entorno.
5. Eliminar AppRole, políticas y secretos Vault solo después de confirmar
   backups y rollback.
6. Retirar `vault/`, `.runtime/vault/` y scripts de migración únicamente cuando
   no existan consumidores y la evidencia de auditoría esté archivada.

## Fase 9 — Rotación y operación continua

- Rotar secretos por servicio, con `AWSCURRENT`/`AWSPREVIOUS`.
- Rotar PostgreSQL sin dejar conexiones huérfanas ni bloquear el servicio.
- Rotar PayPal, R2, Qdrant, OpenRouter y Resend con prueba de conectividad.
- Renovar certificados Private CA antes del margen mínimo.
- Validar KMS con `DescribeKey` y una operación real controlada de Encrypt/Decrypt
  para el `auth-service`.
- Publicar dashboards de lecturas, errores, latencia, expiración y denegaciones.
- Mantener un procedimiento de rollback por secreto, parámetro y certificado.

### Implementación de recarga dinámica (código)

- `common::ConfigReloadHandle` consulta AWS periódicamente, conserva un
  snapshot versionado con hash, publica cambios por `watch` y mantiene el
  último snapshot válido durante errores temporales. No existe fallback a
  Vault.
- El intervalo, habilitación y edad máxima del snapshot se controlan con
  `CONFIG_RELOAD_ENABLED`, `CONFIG_RELOAD_INTERVAL_SECONDS` y
  `CONFIG_RELOAD_MAX_STALE_SECONDS`.
- Todos los binarios registran sus pools PostgreSQL y Redis. PostgreSQL valida
  la nueva URL con una conexión de prueba antes de cambiar las opciones para
  conexiones futuras; Redis construye, valida y sustituye el pool
  atómicamente, cerrando el pool anterior después del intercambio.
- Los bundles mTLS ya se recargan por TTL y margen de validez mediante
  `ServiceTlsConfig::material()`. Los clientes que crean un canal por llamada
  reciben el material vigente.
- `connect_nats` usa un callback de autenticación que lee las credenciales
  vigentes en cada conexión inicial y reconexión. `register_nats_client`
  supervisa el snapshot, actualiza el pool de servidores y solicita una
  reconexión controlada cuando cambia la URL o la credencial, sin recrear los
  contextos JetStream.
- Los listeners gRPC usan `serve_with_incoming_shutdown` sobre un listener
  persistente. Cada conexión TCP crea un `TlsAcceptor` con el material validado
  vigente, por lo que una renovación de certificado se aplica a conexiones
  nuevas sin rebinding ni reinicio; las conexiones existentes terminan con su
  handshake original y se drenan mediante el shutdown normal.

**Estado:** recarga polling/cache, PostgreSQL, Redis, NATS y listeners gRPC
implementados en código. El gate E2E de rotación, redelivery, drenaje y
renovación en caliente sigue siendo obligatorio antes de producción.

## Fase 10 — Pruebas E2E, resiliencia y salida

Ejecutar en staging con dependencias reales:

1. Arranque limpio sin Vault ni `.runtime/vault`.
2. Login, MFA, cifrado/descifrado KMS y recuperación de sesión.
3. Lectura de configuración desde SSM y cambio controlado de un parámetro.
4. Rotación de contraseña de PostgreSQL y recuperación sin pérdida de datos.
5. Lectura de R2, Qdrant, OpenRouter y PayPal desde Secrets Manager.
6. Renovación mTLS sin aceptar certificados de otro servicio.
7. Revocación de permisos IAM y verificación de fallo cerrado.
8. Caída temporal de Secrets Manager/SSM y recuperación por backoff sin
   registrar secretos.
9. Pruebas de carga y carrera durante renovación de secretos/certificados.
10. Verificación de que el Admin Dashboard sigue administrando PostgreSQL y no
    obtiene acceso a secretos de infraestructura.

## Estrategia de rollback

El rollback debe poder ejecutarse por servicio:

1. Promover `AWSPREVIOUS` del secreto o parámetro.
2. Restaurar la versión anterior del bundle TLS.
3. Reiniciar únicamente el servicio afectado si la carga dinámica no está
   disponible.
4. No se revierte al proveedor anterior: se restaura la versión AWS anterior
   y se reinicia el servicio afectado.
5. No reactivar Vault después de retirar sus claves; cualquier restauración
   debe hacerse desde el backup AWS validado.

## Criterios de salida

- Cero servicios que lean Vault en producción.
- Cero secretos en `.env`, imágenes, logs, frontend o PostgreSQL de negocio.
- Cada servicio tiene un rol IAM mínimo y una prueba negativa de permisos.
- Cada parámetro tiene propietario, entorno, tipo, versión y rollback.
- Cada secreto tiene rotación, auditoría CloudTrail y versión anterior utilizable.
- KMS MFA valida permisos reales durante el arranque o health check controlado.
- mTLS usa Private CA/ACM, renueva antes de expirar y reconstruye listeners o
  canales cuando corresponde.
- Cambiar un parámetro técnico no requiere recompilar ni modificar el frontend;
  cambiar reglas de negocio continúa realizándose desde Admin BFF/PostgreSQL.
- `cargo check --workspace`, pruebas E2E, pruebas de aislamiento, pruebas de
  rotación y smoke tests post-despliegue pasan con artefactos verificables.
