# AI Gateway — Seguridad: checklist y prompt para el agente

> Reconciliado con el workspace actual el 2026-07-25. El sistema tiene
> `api-gateway`, `customer-bff`, `admin-bff`, `file-service` y `s3-gateway`;
> los pendientes operativos se validan en staging y no se declaran ejecutados
> sin artefactos de verificación.

> La sección **"Prompt de seguridad para el agente"** se usa junto con los
> documentos de `doc/`, `AGENTS.md`, las migraciones vigentes y los gates de
> `artifacts/verify/`. `02-schema-base-datos.sql` es una referencia histórica;
> las migraciones de cada servicio son el contrato actual.

## 1. Credenciales AWS (lo más crítico del sistema)

- **Nunca** guardes Access Keys de AWS en texto plano en Postgres, logs, variables de entorno del repo, ni en el frontend.
- Preferir **STS AssumeRole** con credenciales temporales (15 min–1 h) sobre Access Keys estáticas. Esto elimina por completo el riesgo de una key permanente filtrada.
- Si en algún caso necesitas Access Keys estáticas (proveedores que no soportan AssumeRole), cífralas con **AWS KMS** (envelope encryption), nunca con una clave simétrica embebida en el código.
- Usa `external_id` en el `AssumeRole` cuando la cuenta AWS del pool no sea tuya (mitiga el ataque de "confused deputy").
- Aplica el **principio de menor privilegio** en el IAM Role: solo `bedrock:InvokeModel` / `bedrock:InvokeModelWithResponseStream` sobre los modelos concretos que vas a revender, nada más.
- Rota las credenciales STS antes de su expiración con margen (no esperes al último segundo, ten un buffer de refresh).

## 2. API keys de tus clientes (tráfico de negocio)

- Genera keys con **suficiente entropía** (mínimo 256 bits aleatorios), formato `prefix + secret`.
- Guarda solo el **hash** (HMAC-SHA256 con un *pepper* del lado del servidor, no solo SHA-256 plano — el pepper evita que un dump de la tabla `api_keys` sea suficiente para fuerza bruta offline si el algoritmo de hash quedara alguna vez comprometido).
- Muestra la key completa **una sola vez**, en el momento de creación. Después, solo el `key_prefix` enmascarado (`sk-live-a1b2****`).
- Compara hashes con comparación en **tiempo constante** (evita timing attacks) — la mayoría de librerías de hash ya lo hacen internamente, pero verifícalo si comparas manualmente.
- Revocación **inmediata**: al revocar una key, invalida también la entrada en el cache de Redis, no dependas solo del TTL.
- Rate-limitea también la **creación** de API keys por usuario (evita que una cuenta comprometida genere cientos de keys).

## 3. Prevención de fraude y abuso de saldo

- **Reserva optimista de saldo antes de llamar a Bedrock** (ver flujo de facturación en el documento de arquitectura). Sin esto, peticiones concurrentes desde una misma key pueden dejar el balance negativo — es la forma más común de "robo" en este tipo de sistemas.
- Verifica el conteo de tokens **del lado del servidor** (lo que devuelve Bedrock en `usage`), nunca confíes en un valor que el cliente pueda reportar.
- Aplica límites de `max_tokens` por plan del lado del gateway, no solo confíes en lo que el cliente envía en el body.
- Detecta patrones anómalos (ej. una key con picos de gasto muy por encima de su histórico) y dispara alertas — no necesitas ML para esto al inicio, un umbral simple sobre `usage_hourly` ya ayuda.
- Idempotencia obligatoria en toda operación que mueva dinero (top-ups, ajustes manuales, reembolsos) usando `idempotency_key`.

## 4. Seguridad entre microservicios (interna)

- Autenticación **servicio-a-servicio**: no dejes los puertos gRPC internos abiertos sin autenticar. Usa mTLS entre servicios o, como mínimo, tokens firmados (JWT de servicio) validados en cada llamada interna.
- Los servicios internos (`model-router`, `credential-manager`, `billing-service`, etc.) **no deben ser accesibles desde internet** — solo dentro de la red privada/VPC, con security groups restrictivos.
- Cada microservicio conecta a su **base PostgreSQL independiente** de menor
  privilegio dentro del contenedor compartido; no se usan esquemas compartidos.
- Namespacing estricto de claves en Redis por servicio (`ratelimit:{key_id}`, `tpm:{account_id}`) para evitar colisiones o fugas entre usuarios.

## 5. Prevención de fugas y validaciones básicas de backend

- **SQL injection:** usa siempre queries parametrizadas (`sqlx`/`sea-orm`), nunca interpolación de strings en SQL.
- **Nunca loguees** API keys completas, secretos, tokens de sesión, ni datos de tarjetas — enmascara (`sk-live-a1b2****`) antes de escribir a logs o a tu sistema de observabilidad.
- **PCI compliance:** nunca proceses ni almacenes el número completo de tarjeta
  (PAN) en tu backend. El único proveedor de pagos actual es PayPal Checkout;
  la base solo guarda referencias tokenizadas del proveedor.
- Verifica la **firma HMAC de los webhooks entrantes** de tu pasarela de pago antes de procesarlos (evita que alguien falsifique un `payment.succeeded`).
- Verifica la firma de **tus propios webhooks salientes** hacia clientes (firma HMAC con el `signing_secret` por endpoint) para que ellos puedan validar autenticidad.
- Protege contra **SSRF** si en algún momento permites URLs configurables por el usuario (ej. webhook endpoints): valida que no apunten a IPs internas/metadata de AWS (`169.254.169.254`).
- Cifra en reposo los campos sensibles (`mfa_secret`, `signing_secret`, credenciales estáticas) con KMS; TLS 1.2+ obligatorio en tránsito en todo el sistema, interno y externo.
- Define una política de **retención y borrado** de `usage_events` crudos (cumplimiento y costo de almacenamiento) — archiva a S3/Glacier antes de eliminar particiones.

## 6. Rate limiting y disponibilidad

- Rate limiting en **múltiples niveles**: por API key, por usuario, y un límite global de protección del sistema (evita que un solo cliente degrade el servicio para todos).
- Circuit breaker por cuenta AWS (ya cubierto en arquitectura) — evita seguir enviando tráfico a una cuenta que está siendo throttled.
- Considera un WAF / protección anti-DDoS en el borde (Cloudflare, AWS WAF) delante de `api-gateway`.
- Backpressure explícito: si `model-router` no encuentra ninguna cuenta sana para un modelo, responde `503` con `Retry-After`, no dejes la petición colgada.

## 7. Frontend (dashboard de clientes y panel admin)

- **Nunca** guardes el JWT de sesión en `localStorage`/`sessionStorage` — usa cookies `HttpOnly` + `Secure` + `SameSite=Strict`, gestionadas por el BFF (esto es justamente para qué sirve el patrón BFF).
- **CSRF:** si usas cookies de sesión, implementa protección CSRF (double-submit token o `SameSite=Strict` como primera línea de defensa) en todos los endpoints mutables del BFF.
- **CSP (Content-Security-Policy)** estricta: evita inline scripts sin nonce, restringe `connect-src` a tus propios dominios.
- La API key completa se muestra **una sola vez** tras crearla, con un botón "copiar" — después de refrescar la página o navegar, ya no debe ser recuperable, solo el prefijo.
- Sanitiza cualquier contenido que renderices que provenga de una respuesta de modelo (si tienes un "playground" en el dashboard) para evitar XSS almacenado.
- Confirmaciones explícitas (modal, no solo un botón) para acciones destructivas: revocar key, eliminar cuenta, cambiar de plan a uno más restrictivo.
- El **panel admin** debe vivir en un subdominio/dominio separado del dashboard de clientes, sin compartir cookies de sesión, y con MFA obligatorio para todos los operadores internos.
- Formularios de pago: usa siempre el checkout/widget hospedado del proveedor. El frontend nunca debe tocar el número de tarjeta en JS plano.
- Rate limit de intentos de login del lado del backend (no solo UX del lado del frontend) para prevenir fuerza bruta sobre `auth-service`.

## 8. Observabilidad orientada a seguridad

- `audit.audit_logs` debe registrar como mínimo: creación/revocación de API keys, cambios de plan, ajustes manuales de balance, cambios de pricing, altas/bajas de cuentas AWS del pool, y todo login administrativo.
- Alertas automáticas sobre: múltiples cuentas AWS marcadas `unhealthy` en poco tiempo (posible incidente en el pool), picos de `402 Payment Required` (puede indicar un ataque de agotamiento de saldo), tasa de error elevada en un modelo/cuenta específicos.

---

## Prompt de seguridad para el agente

```
Al construir cada microservicio de este sistema, aplica de forma no negociable:

1. Ninguna credencial de AWS (Access Key, Secret Key) se persiste en texto plano.
   Prioriza STS AssumeRole con credenciales temporales sobre Access Keys estáticas.
   Si se usan Access Keys estáticas, cífralas con KMS antes de persistir.

2. Las API keys de clientes se generan con un mínimo de 256 bits de entropía,
   se muestran completas una sola vez, y en base de datos solo se guarda su
   hash (HMAC-SHA256 con pepper del servidor). La comparación de hashes debe
   ser en tiempo constante.

3. Todo endpoint que mueva saldo o dinero debe ser idempotente (usa una tabla
   de idempotency_keys) y debe usar el patrón de reserva optimista de saldo
   antes de ejecutar la operación costosa (llamada a Bedrock), liberando o
   ajustando la reserva después según el resultado real.

4. Ninguna query a Postgres se construye por interpolación de strings; usa
   siempre parámetros bindeados.

5. Nunca loguees valores completos de API keys, tokens de sesión, secretos de
   webhook, ni datos de tarjetas. Enmascara antes de loguear.

6. La comunicación entre microservicios internos (gRPC) requiere autenticación
   servicio-a-servicio (mTLS o JWT de servicio); ningún puerto interno debe
   quedar expuesto a internet.

7. Cada microservicio se conecta a Postgres con un rol dedicado, con permisos
   limitados únicamente a su propia base de datos del servicio.

8. El frontend (dashboard y panel admin) nunca almacena tokens de sesión en
   localStorage/sessionStorage; usa cookies HttpOnly + Secure + SameSite=Strict
   gestionadas por el BFF correspondiente, con protección CSRF en endpoints
   mutables y una política CSP estricta.

9. Los formularios de pago usan exclusivamente el checkout/widget hospedado del
   proveedor; el backend nunca recibe ni persiste el número completo de tarjeta.

10. Toda acción sensible (creación/revocación de keys, cambios de plan,
    ajustes de balance, cambios de pricing, altas/bajas de cuentas AWS) debe
    quedar registrada en audit_logs con actor, acción, recurso y metadata.

Antes de dar por cerrada la implementación de cada servicio, verifica
explícitamente estos 10 puntos contra el código generado y señala cualquier
punto que no se haya podido cumplir y por qué.
```
