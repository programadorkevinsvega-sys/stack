# Guía general de seguridad para frontend

Documento reutilizable para aplicaciones web de cualquier lenguaje, framework,
arquitectura o proveedor de hosting. No sustituye una revisión de seguridad
del backend, del navegador ni de la infraestructura.

## 1. Alcance y modelo de confianza

El código que se ejecuta en el navegador debe considerarse público y
modificable por el usuario. Un atacante puede desactivar JavaScript, modificar
el bundle, repetir peticiones, cambiar cabeceras o llamar directamente a la API.

Por tanto:

- El frontend puede validar datos para mejorar la experiencia, pero nunca es la
  autoridad de autenticación, autorización, cuotas, precios o permisos.
- Toda regla de seguridad se repite y se aplica de forma autoritativa en el
  backend.
- El frontend no debe recibir secretos que permitan operar como el servidor.
- Los mensajes de error mostrados al usuario no deben revelar secretos,
  consultas, trazas internas ni información de otros usuarios.

## 2. Prevención de XSS

El objetivo es impedir que datos controlados por un usuario se interpreten como
HTML o JavaScript en el navegador de otra persona.

- Renderiza texto mediante las APIs normales del framework, que escapan el
  contenido por defecto.
- Evita APIs equivalentes a `dangerouslySetInnerHTML` salvo que exista una
  necesidad justificada y el contenido haya sido sanitizado con una librería
  mantenida y configurada con una lista permitida mínima.
- No interpolar valores no confiables dentro de HTML, CSS, JavaScript, SVG,
  plantillas o atributos sensibles.
- No construir un enlace a partir de una cadena no validada. Permite solo
  destinos esperados (`https` y, cuando proceda, `http` en desarrollo) y
  rechaza esquemas como `javascript:`, `data:` o `vbscript:`.
- Trata el contenido procedente de documentos, comentarios, perfiles, nombres
  de archivo y respuestas de API como no confiable.
- No confíes en que ocultar un elemento o deshabilitar un botón protege una
  operación; el servidor debe comprobar el permiso.

## 3. Content Security Policy (CSP)

CSP limita los orígenes y tipos de recursos que el navegador puede cargar. Debe
ser una cabecera HTTP, no una medida implementada solo desde JavaScript.

Una política inicial debe ser restrictiva y ajustarse a los recursos reales de
la aplicación:

```text
default-src 'self';
script-src 'self';
style-src 'self';
img-src 'self' data: https:;
font-src 'self' https:;
connect-src 'self' https://api.example.invalid;
object-src 'none';
base-uri 'self';
frame-ancestors 'none';
form-action 'self';
```

Recomendaciones:

- Empieza con `Content-Security-Policy-Report-Only` para descubrir recursos
  legítimos y luego aplica la política obligatoria.
- No agregues `unsafe-eval` o `unsafe-inline` para ocultar un problema de
  configuración. Si una dependencia los exige, evalúa reemplazarla o aíslarla.
- Cada script de terceros debe estar justificado, inventariado y limitado al
  origen exacto que necesita.
- Si se usa un proveedor de analítica, su origen debe declararse expresamente
  en `script-src` y `connect-src`, respetando sus implicaciones de privacidad.

## 4. Cabeceras HTTP de protección

Configura estas cabeceras en el servidor, CDN o proxy que entrega la aplicación:

- `Strict-Transport-Security: max-age=31536000; includeSubDomains` después de
  confirmar que todos los subdominios funcionan exclusivamente con HTTPS.
- `X-Content-Type-Options: nosniff` para impedir interpretaciones MIME
  inesperadas.
- `Referrer-Policy: strict-origin-when-cross-origin` o una política más
  restrictiva según el caso.
- `Permissions-Policy` deshabilitando sensores, cámara, micrófono,
  geolocalización y otras capacidades que la aplicación no necesita.
- `Content-Security-Policy` con `frame-ancestors 'none'` para evitar que la
  aplicación se incruste en un iframe de un sitio malicioso.
- `X-Frame-Options: DENY` como compatibilidad con navegadores antiguos cuando
  no exista un requisito legítimo de incrustación.

No añadas cabeceras que contradigan el diseño de la aplicación. Si se requiere
incrustación, usa una lista explícita de orígenes permitidos y no `*`.

## 5. Validación de datos: UX frente a seguridad

La validación en el cliente sirve para dar feedback inmediato: formato de
correo, campos vacíos, longitud, rango, formato de archivo o confirmación de un
formulario. No es un control de seguridad.

El backend debe volver a validar:

- tipo, tamaño, codificación y estructura;
- autenticación, autorización y pertenencia del recurso;
- límites de negocio, precios, moneda y cuotas;
- nombres, rutas, filtros y parámetros de consulta;
- contenido y tipo real de archivos, no solo su extensión;
- idempotencia y repetición de operaciones.

Los esquemas declarativos del frontend (por ejemplo, validadores de tipos) son
útiles para UX y contratos, pero no sustituyen la validación del servidor.

## 6. Autenticación, sesiones y cookies

La sesión debe diseñarse con el backend como fuente de verdad.

- Usa cookies `HttpOnly` para credenciales que no necesitan ser leídas por
  JavaScript.
- Usa `Secure` siempre en producción y `SameSite=Lax` o `SameSite=Strict`
  cuando el flujo lo permita.
- Si una cookie debe enviarse en un contexto cross-site, utiliza `SameSite=None`
  junto con `Secure` y documenta el motivo.
- No guardes tokens, contraseñas ni secretos en `localStorage`, `sessionStorage`,
  URLs, fragmentos, logs del navegador o datos analíticos.
- Protege las mutaciones autenticadas contra CSRF mediante `SameSite`, token
  antifalsificación, comprobación de `Origin`/`Referer` o una combinación
  apropiada.
- El logout debe invalidar la sesión en el servidor; borrar una cookie local no
  revoca por sí mismo un token ya emitido.
- El servidor debe poder revocar sesiones, cerrar todas las sesiones de una
  cuenta y rechazar sesiones expiradas o revocadas sin esperar únicamente a la
  expiración natural de un token.
- Ante un `401`, elimina el estado de autenticación del cliente, evita bucles de
  reintento y redirige a un destino interno seguro.
- No incluyas tokens en parámetros de URL, enlaces de correo visibles,
  cabeceras de referer ni mensajes de error.

## 7. Prevención de open redirect

Las redirecciones posteriores a login, logout, registro o pago deben aceptar
solo rutas internas relativas, por ejemplo `/panel`.

- Rechaza URLs absolutas, esquemas alternativos, valores codificados de forma
  ambigua y destinos fuera de una lista permitida.
- Normaliza y valida el destino en el servidor, no solo en el router del
  navegador.
- Define un destino predeterminado seguro si el parámetro falta o es inválido.

## 8. Secretos y configuración pública

Todo lo que se incluye en el bundle del navegador puede ser inspeccionado.

Nunca envíes al cliente:

- claves privadas de proveedores;
- credenciales de bases de datos, colas, almacenamiento o servicios internos;
- secretos de firma, contraseñas, tokens de administración o claves de cifrado;
- permisos o endpoints internos que no sean necesarios.

Una clave pública diseñada para el navegador no debe confundirse con un secreto.
La configuración pública debe limitarse a valores no sensibles, y el servidor
debe aplicar igualmente todos los controles.

## 9. Scripts y dependencias de terceros

- Mantén un inventario de scripts externos, propietario, finalidad, origen y
  datos que reciben.
- Prefiere paquetes instalados y versionados sobre scripts dinámicos cargados
  sin control.
- Para recursos CDN que lo permitan, usa Subresource Integrity (`integrity`)
  junto con `crossorigin="anonymous"`.
- Revisa cambios de dependencias y ejecuta auditorías en CI (`npm audit`,
  `pnpm audit`, `yarn audit`, `cargo audit` u otra herramienta equivalente).
- Fija versiones, elimina dependencias sin uso y revisa paquetes abandonados.
- No cargues scripts desde dominios comodín ni permitas que un parámetro del
  usuario elija el origen de un script.

## 10. Archivos, contenido y descargas

El navegador no puede decidir si un archivo es seguro. Para cualquier subida:

- valida tamaño, tipo declarado, tipo detectado y nombre en el backend;
- usa URLs de subida de corta duración y con alcance mínimo cuando exista
  almacenamiento de objetos;
- no expongas rutas privadas ni credenciales de almacenamiento;
- muestra el estado de procesamiento sin insertar el contenido como HTML;
- descarga con `Content-Disposition` y tipos MIME controlados por el servidor;
- aplica autorización al listar, leer, renombrar, mover y eliminar cada objeto;
- trata nombres y metadatos de archivos como texto no confiable.

Si se permite previsualizar HTML, SVG, PDF u otros formatos activos, hazlo en un
origen aislado o mediante un proceso de sanitización y transformación seguro.

## 11. Clickjacking y navegación

- Bloquea la incrustación con `frame-ancestors 'none'` o una lista explícita.
- Añade confirmación visible para acciones destructivas, pero no la uses como
  sustituto de autorización o protección CSRF.
- Deshabilita enlaces y botones mientras se procesa una acción para evitar
  dobles envíos accidentales; el backend debe ser idempotente.
- No uses `window.open` con destinos no confiables sin una política clara de
  `noopener` y validación del destino.

## 12. Privacidad y observabilidad

- No envíes PII innecesaria a analítica, logs del navegador, trazas o
  herramientas de repetición de sesión.
- Enmascara correos, teléfonos, identificadores y contenido sensible cuando no
  sean necesarios para diagnosticar un problema.
- No registres tokens, cookies, contraseñas, encabezados de autorización ni
  cuerpos completos de formularios.
- Define retención, acceso y eliminación para los datos de telemetría.
- Los mensajes de error para el usuario deben ser útiles sin revelar detalles
  internos.

## 13. Rate limiting y protección contra repetición

Debounce, throttle y deshabilitar botones son mejoras de UX; no protegen una API
contra un atacante que envía peticiones directamente.

El backend o gateway debe aplicar rate limiting, límites de concurrencia,
idempotency keys y controles antifraude para login, pagos, recuperación de
cuenta, subida de archivos y acciones costosas.

## 14. Pruebas mínimas antes de producción

- Buscar usos de HTML dinámico, URLs construidas con entrada externa y scripts
  de terceros.
- Verificar CSP, HSTS, cookies, CSRF, clickjacking y tipos MIME con una
  herramienta de cabeceras y una prueba manual.
- Intentar acceder a una ruta protegida sin sesión, con sesión expirada y con
  sesión revocada.
- Confirmar que cerrar sesión invalida la sesión en el servidor y que no se
  puede reutilizar el historial o un token anterior.
- Probar open redirects con destinos absolutos, esquemas peligrosos y valores
  codificados.
- Intentar modificar desde DevTools los límites, precios, roles, identificadores
  y permisos enviados por el cliente; el servidor debe ignorarlos o rechazarlos.
- Ejecutar pruebas de XSS almacenado y reflejado con datos de prueba controlados.
- Revisar dependencias y bloquear vulnerabilidades críticas y altas según la
  política del proyecto.
- Probar navegadores y tamaños de pantalla soportados, además de navegación por
  teclado y lector de pantalla en los flujos críticos.

## 15. Lista de salida

Antes de publicar una versión, confirma:

- [ ] Todo el tráfico público usa HTTPS.
- [ ] HSTS está habilitado cuando el dominio y sus subdominios están listos.
- [ ] CSP está aplicada y no contiene orígenes innecesarios.
- [ ] No hay secretos en el código, bundle, mapas de origen públicos ni
      variables expuestas al cliente.
- [ ] Las cookies de sesión tienen atributos seguros y existe protección CSRF.
- [ ] El logout revoca la sesión en el servidor.
- [ ] Las redirecciones aceptan únicamente destinos internos.
- [ ] El HTML dinámico se escapa o sanitiza antes de renderizarse.
- [ ] Las subidas y descargas están autorizadas y limitadas en el backend.
- [ ] Las dependencias fueron auditadas y los scripts externos inventariados.
- [ ] No se registran tokens ni PII innecesaria.
- [ ] Existen pruebas automatizadas para los flujos autenticados y destructivos.

## 16. Principio final

El frontend es una interfaz de usuario y una superficie de ataque, no una
frontera de confianza. La validación local, la ocultación de botones y el
rate-limit del navegador mejoran la experiencia, pero la seguridad real debe
ser verificable y exigible en el servidor.
