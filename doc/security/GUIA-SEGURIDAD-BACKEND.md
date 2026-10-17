# Guía general de seguridad para backend

Guía reutilizable para APIs, servicios web, workers, gateways y procesos
asíncronos, sin depender de un lenguaje, framework, proveedor cloud o motor de
base de datos concreto.

La seguridad backend no consiste en añadir una librería de autenticación. Es un
sistema de controles que debe proteger confidencialidad, integridad,
disponibilidad, privacidad y trazabilidad durante todo el ciclo de vida de una
petición.

Como referencias de verificación usa OWASP ASVS, OWASP API Security Top 10,
CWE Top 25 y un marco de desarrollo seguro como NIST SSDF o equivalente. Las
listas no sustituyen el modelado de amenazas: sirven para comprobar que no se
omitió una categoría de control.

## 1. Modelo de amenazas y diseño seguro

Antes de escribir código, documenta:

- activos protegidos y su impacto si se filtran, alteran o destruyen;
- actores, límites de confianza y superficies públicas;
- identidades, roles, recursos y acciones permitidas;
- dependencias internas y externas;
- caminos de datos, secretos y operaciones irreversibles;
- supuestos de disponibilidad, latencia y recuperación;
- abuso esperado, no solo el uso feliz.

Usa modelado de amenazas con STRIDE, attack trees o un método equivalente.
Cada riesgo debe tener una mitigación verificable, un propietario y una prueba.
Revisa el modelo cuando cambien contratos, permisos, infraestructura o flujos
de dinero.

Principios base:

- denegación por defecto;
- mínimo privilegio;
- separación de responsabilidades;
- validación en cada frontera de confianza;
- no confiar en datos enviados por el cliente ni por servicios internos;
- fallar de forma segura y sin filtrar información sensible;
- hacer operaciones críticas idempotentes y auditables.

## 2. Autenticación

La autenticación debe verificar una identidad antes de permitir cualquier
operación protegida.

- Usa protocolos y librerías mantenidas; no implementes criptografía ni
  verificación de tokens a mano.
- Valida firma, algoritmo permitido, emisor, audiencia, expiración, no antes de
  y todos los claims necesarios.
- No aceptes un token firmado con un algoritmo distinto al configurado.
- Define expiración corta para credenciales de acceso y renovación controlada.
- Permite revocación inmediata o validación contra estado de sesión cuando el
  riesgo lo requiera.
- Protege recuperación de cuenta, verificación de correo y cambio de
  credenciales con tokens aleatorios, de un solo uso, con alcance y expiración.
- Usa MFA para cuentas administrativas y acciones de alto impacto.
- Aplica protección contra enumeración de usuarios en login y recuperación.
- Compara errores de autenticación sin revelar si existe una cuenta.
- Registra intentos y aplica límites progresivos sin bloquear permanentemente a
  un usuario por una sola dirección de red.

Las contraseñas deben almacenarse con un algoritmo adaptativo moderno, sal única
por credencial y parámetros revisados (por ejemplo, Argon2id o equivalente).
Nunca las cifres esperando descifrarlas después ni las registres en logs.

## 3. Autorización y control de acceso

Autenticación responde quién es el sujeto; autorización responde qué puede
hacer sobre qué recurso y bajo qué condición.

- Comprueba autorización en cada operación protegida, no solo al entrar en una
  pantalla o grupo de rutas.
- Deriva el propietario del contexto autenticado; no aceptes `user_id`, tenant,
  rol o permisos enviados por el cliente como autoridad.
- Aplica controles a nivel de objeto, función y campo.
- Evita confiar únicamente en roles gruesos; considera políticas por recurso,
  estado, organización, región, plan o relación de propiedad.
- Rechaza por defecto permisos ausentes, ambiguos o desconocidos.
- Separa permisos de lectura, creación, actualización, eliminación,
  administración y exportación.
- Comprueba autorización también en tareas asíncronas, jobs, webhooks y
  endpoints internos.
- Invalida cachés de autorización cuando cambien roles, membresías o sesiones.
- No conviertas un identificador predecible en autorización; usa controles
  explícitos contra IDOR/BOLA.

Las pruebas deben incluir siempre dos identidades: una intenta operar sobre los
recursos de la otra y la operación debe ser rechazada sin filtrar metadatos.

## 4. Contratos y validación de entradas

Valida cada entrada en el servidor con esquemas explícitos:

- tipo, tamaño, codificación y campos obligatorios;
- valores permitidos, rangos, precisión y unidades;
- fechas, zonas horarias, monedas y estados válidos;
- paginación, ordenamiento y filtros mediante listas permitidas;
- límites de profundidad, cantidad de elementos y tamaño del cuerpo;
- compatibilidad de versión y campos desconocidos.

Rechaza entradas inválidas de forma consistente. No conviertas silenciosamente
un valor inválido en uno permitido ni aceptes campos desconocidos que puedan
activar comportamientos futuros.

La validación de formato no basta: una entrada con forma correcta puede no
estar autorizada o ser maliciosa.

## 5. Inyección y acceso a datos

- Usa consultas parametrizadas o APIs de consulta seguras.
- No concatenes SQL, comandos, expresiones NoSQL, rutas, plantillas ni filtros
  con entrada del usuario.
- Permite nombres de columnas, tablas, campos de ordenamiento y operadores solo
  desde listas internas.
- Escapa datos al construir expresiones que no admitan parámetros, documentando
  el caso y probándolo explícitamente.
- Usa usuarios de base de datos con mínimo privilegio.
- Limita filas, tiempo y memoria de consultas expuestas a usuarios.
- Protege transacciones contra carreras, doble débito, doble uso y estados
  parciales.
- Define constraints e invariantes en la base además de validarlas en código.
- Evita devolver errores del motor, planes de ejecución o consultas al cliente.

Revisa también inyección en LDAP, XPath, expresiones de búsqueda, motores de
plantillas, shell, correo, logs, cabeceras y sistemas de colas.

## 6. APIs HTTP, RPC y mensajería

### HTTP y RPC

- Exige TLS y valida certificados en comunicaciones internas y externas.
- Define límites de cuerpo, cabeceras, tiempo, conexiones y concurrencia.
- Usa códigos de estado coherentes y respuestas de error sin detalles internos.
- Valida `Content-Type`, método, versión, host y origen cuando corresponda.
- Deshabilita métodos, rutas y formatos que no se utilicen.
- Evita diferencias observables que permitan enumerar recursos.
- Usa paginación y límites máximos; nunca devuelvas colecciones ilimitadas.
- Protege métodos de mutación contra CSRF cuando se autentiquen con cookies.

### Eventos y colas

- Autentica productores y consumidores.
- Valida versión, esquema, tamaño y origen de cada mensaje.
- Usa identificadores de evento e idempotencia para soportar redelivery.
- Define reintentos con backoff, límite, DLQ y procedimiento de recuperación.
- No incluyas secretos o PII innecesaria en eventos.
- Rechaza mensajes desconocidos o incompatibles sin procesarlos parcialmente.
- Protege contra replay con expiración, nonce o registro de eventos procesados.

## 7. Protección contra amenazas web y de red

### SSRF

Las funciones que descargan URLs, validan webhooks o consultan recursos remotos
deben:

- permitir solo esquemas y destinos necesarios;
- resolver DNS y validar la dirección final, no solo el texto recibido;
- bloquear loopback, redes privadas, metadatos cloud, multicast y rangos
  reservados;
- limitar redirecciones y revalidar cada salto;
- usar timeouts, límites de tamaño y número de respuestas;
- separar la red de salida del proceso que recibe entrada pública.

### Deserialización y plantillas

- No deserialices tipos arbitrarios controlados por el usuario.
- Usa formatos con esquemas y listas de tipos permitidos.
- Firma o autentica mensajes cuando la integridad sea importante.
- No evalúes plantillas, expresiones o código generado desde entrada externa.

### Cabeceras y proxies

- Normaliza cabeceras antes de tomar decisiones de seguridad.
- Define una política de proxies confiables para IP original, host y esquema.
- No uses `X-Forwarded-For` sin validar qué proxy lo estableció.
- Protege contra request smuggling con configuraciones consistentes entre CDN,
  proxy y aplicación.

## 8. Archivos y contenido no confiable

- Limita tamaño, cantidad, duración de procesamiento y cuota por identidad.
- Valida el tipo real, no solo extensión o `Content-Type` declarado.
- Cambia nombres y evita usar rutas proporcionadas por el usuario.
- Almacena archivos fuera de la raíz ejecutable y con autorización por objeto.
- Escanea malware cuando el riesgo y el tipo de archivo lo requieran.
- Deshabilita ejecución de contenido subido.
- Procesa PDF, imágenes, hojas de cálculo y archivos comprimidos en aislamiento,
  con límites de CPU, memoria, recursión y tiempo.
- Evita zip bombs, descompresión ilimitada y archivos con enlaces simbólicos.
- Entrega descargas con MIME y `Content-Disposition` controlados por el servidor.
- Usa URLs firmadas de alcance y duración mínima, sin exponer credenciales de
  almacenamiento.

## 9. Criptografía y manejo de secretos

- Usa bibliotecas criptográficas mantenidas y primitivas estándar.
- Genera aleatoriedad con un CSPRNG del sistema.
- Usa AEAD para cifrado autenticado y hash adaptativo para contraseñas.
- Separa claves por propósito, entorno y nivel de sensibilidad.
- No reutilices claves de firma para cifrado ni claves de entornos distintos.
- Rota claves con versión, período de solapamiento y revocación documentados.
- Protege claves maestras en KMS/HSM o gestor de secretos.
- No pongas secretos en código, imágenes, variables públicas, URLs, eventos ni
  logs.
- Redacta secretos en errores, trazas y volcado de diagnósticos.

## 10. Sesiones, tokens y cookies

- Usa cookies `HttpOnly` y `Secure` para credenciales que no necesita leer el
  cliente.
- Configura `SameSite` según el flujo y añade protección CSRF para mutaciones.
- No aceptes tokens por múltiples ubicaciones ambiguas sin una razón explícita.
- No pongas credenciales en URLs ni las escribas en logs.
- Revoca sesiones en logout, cambio de contraseña, MFA comprometido y
  acciones administrativas relevantes.
- Registra sesiones activas y ofrece cierre individual o global cuando el
  producto lo requiera.
- Evita refresh tokens indefinidos; rota y detecta reutilización.
- Valida audiencias, emisores, scopes, expiración y versión de credencial.

## 11. Lógica de negocio y abuso

Los fallos más costosos suelen estar en flujos válidos usados de forma indebida.

- Define invariantes de saldo, inventario, cuotas, estados y transiciones.
- Autoriza cada cambio de estado, no solo la creación inicial.
- Usa idempotency keys para pagos, órdenes, reintentos y webhooks.
- Impide reutilizar descuentos, tokens de invitación, códigos y enlaces de un
  solo uso.
- Aplica límites por identidad, cuenta, dispositivo, IP y operación de alto
  riesgo.
- Verifica montos, moneda, destinatario y contexto en el servidor.
- Añade confirmación adicional para acciones irreversibles o sensibles.
- Diseña compensaciones para fallos entre reserva, débito, entrega y registro.

## 12. Dependencias y cadena de suministro

- Fija versiones y revisa cambios de dependencias.
- Escanea vulnerabilidades y licencias en cada cambio y de forma programada.
- Genera un SBOM y conserva artefactos reproducibles cuando sea posible.
- Firma imágenes y binarios y verifica firmas antes de desplegar.
- Limita scripts de instalación y dependencias transitivas innecesarias.
- Usa repositorios confiables y revisa paquetes abandonados o secuestrados.
- Separa credenciales de CI por entorno y concede permisos temporales.
- Protege el pipeline contra ejecución de código de pull requests no confiables.

## 13. Errores, logs y observabilidad

- Devuelve mensajes genéricos al cliente y detalles solo a personal autorizado.
- Usa identificadores de solicitud para correlacionar sin exponer datos sensibles.
- No registres contraseñas, tokens, cookies, claves, cuerpos completos ni datos
  financieros.
- Redacta PII y establece retención, acceso y borrado de telemetría.
- Registra cambios de privilegios, fallos de autenticación, denegaciones,
  operaciones destructivas y anomalías de uso.
- Protege logs contra modificación y acceso por el mismo rol que genera eventos.
- Evita que un atacante pueda llenar el almacenamiento de logs con peticiones
  pequeñas repetidas.
- Alerta sobre picos de errores, 401/403, 429, fallos de integridad, DLQ,
  reintentos y cambios administrativos.

## 14. Disponibilidad y resiliencia

- Define timeouts para cada dependencia y no permitas esperas indefinidas.
- Aplica límites de concurrencia, colas acotadas y backpressure.
- Usa circuit breakers y reintentos solo para errores transitorios, con jitter
  y un máximo explícito.
- No reintentes operaciones no idempotentes sin una clave de idempotencia.
- Aísla pools y presupuestos para evitar fallos en cascada.
- Controla tamaño de respuestas, memoria, CPU y almacenamiento temporal.
- Protege endpoints de alto costo con cuotas y autenticación adecuada.
- Prueba caída de dependencias, timeouts, particiones, redelivery y recuperación.

La disponibilidad no debe lograrse desactivando autenticación, auditoría o
validación cuando una dependencia falla.

## 15. Administración y operaciones

- Separa endpoints públicos, internos y administrativos.
- Protege administración con MFA, red restringida y permisos separados.
- No expongas depuradores, paneles, documentación sensible o health checks
  detallados en internet.
- Health checks públicos deben revelar solo lo necesario.
- Usa despliegues graduales, migraciones compatibles y rollback probado.
- Rota secretos y certificados sin dejar credenciales antiguas activas más allá
  de la ventana necesaria.
- Revisa accesos y permisos periódicamente.
- Mantén inventario de servicios, puertos, dependencias, propietarios y ciclos
  de soporte.

## 16. Pruebas de seguridad y hacking autorizado

Todas las pruebas ofensivas deben tener alcance, autorización, ambiente y
ventana definidos. Nunca pruebes sistemas de terceros sin permiso explícito.

### Pruebas automáticas

- análisis estático y reglas de seguridad del lenguaje;
- análisis de dependencias, imágenes y configuración;
- escaneo de secretos antes de cada commit y en CI;
- pruebas de contratos y validación de esquemas;
- fuzzing de parsers, deserializadores y endpoints complejos;
- property-based testing para invariantes de negocio;
- pruebas de concurrencia, idempotencia y redelivery.

### Pruebas dinámicas

- autenticación rota y bypass de autorización;
- BOLA/IDOR entre dos identidades;
- inyección SQL, NoSQL, comandos, plantillas y expresiones;
- SSRF, open redirect y request smuggling;
- XSS almacenado o reflejado cuando el backend almacene contenido;
- manipulación de precios, cuotas, roles, estados y cantidades;
- replay, reutilización de tokens y doble ejecución;
- abuso de paginación, límites, compresión y cargas anidadas;
- subida de archivos maliciosos, zip bombs y tipos MIME falsos;
- errores 5xx, trazas, secretos y datos de otros usuarios en respuestas;
- rate limiting, agotamiento de conexiones y backpressure.

### Reglas de calidad de las pruebas

- Cada hallazgo debe incluir reproducción mínima, impacto, evidencia y
  corrección verificable.
- Prueba el rechazo, no solo el caso permitido.
- Repite las pruebas con identidades, roles y estados diferentes.
- No uses datos reales de clientes en pruebas ofensivas.
- Automatiza regresiones de cualquier vulnerabilidad corregida.

## 17. Respuesta a incidentes

Define por adelantado:

- quién declara un incidente y quién decide aislar servicios;
- cómo revocar sesiones, claves, certificados y tokens;
- cómo preservar logs y evidencia sin destruir la trazabilidad;
- cómo bloquear una ruta o una cuenta de forma reversible;
- cómo notificar a usuarios, proveedores y autoridades cuando aplique;
- cómo restaurar datos y verificar integridad;
- cómo documentar causa raíz y acciones preventivas.

Practica simulacros de fuga de secreto, cuenta administrativa comprometida,
exposición de backup, ransomware, dependencia caída y abuso masivo de API.

## 18. Lista de salida a producción

- [ ] Existe un modelo de amenazas actualizado y cada riesgo tiene una prueba.
- [ ] Autenticación, expiración, revocación y recuperación están definidas.
- [ ] Cada operación verifica autorización a nivel de recurso.
- [ ] Las entradas, archivos, consultas y mensajes tienen esquemas y límites.
- [ ] Las comunicaciones usan TLS y validan identidades.
- [ ] No hay secretos en código, imágenes, respuestas, URLs ni logs.
- [ ] Se usan consultas parametrizadas y roles de mínimo privilegio.
- [ ] Existe protección contra SSRF, inyección, replay y abuso de recursos.
- [ ] Los flujos críticos son idempotentes y resistentes a carreras.
- [ ] Hay timeouts, backpressure, límites de concurrencia y recuperación.
- [ ] Dependencias, artefactos e imágenes fueron escaneados y firmados.
- [ ] Los logs no contienen secretos y existe auditoría de acciones sensibles.
- [ ] Se ejecutaron pruebas adversariales autorizadas y se cerraron hallazgos
      críticos y altos.
- [ ] Existe un procedimiento probado de rotación, rollback y respuesta a
      incidentes.

## 19. Principio final

Un backend seguro no presupone que el cliente, la red interna, una dependencia,
un token válido o un usuario autenticado sean confiables para todo. Cada
frontera verifica identidad, autorización, integridad, límites y contexto. La
seguridad se considera terminada solo cuando las defensas están implementadas,
observables y probadas mediante casos negativos y ataques autorizados.
