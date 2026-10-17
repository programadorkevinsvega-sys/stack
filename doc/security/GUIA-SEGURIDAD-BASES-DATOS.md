# Guía general de seguridad para bases de datos

Documento reutilizable para sistemas que utilizan bases de datos relacionales
u otros motores persistentes. Las decisiones concretas dependen del motor, el
proveedor y el nivel de sensibilidad de los datos.

La seguridad de una base de datos debe funcionar por capas: controlar quién se
conecta, qué puede hacer, qué datos puede ver, cómo se cifran, cómo se auditan y
cómo se recuperan después de un incidente.

## 1. Control de acceso y autenticación

### Principio de menor privilegio

- Cada aplicación, servicio, tarea de migración y herramienta administrativa
  debe tener una identidad distinta.
- Una identidad de aplicación solo recibe permisos sobre las bases, esquemas,
  tablas y operaciones que realmente necesita.
- No utilices un superusuario para el tráfico normal de la aplicación.
- Separa las identidades de lectura, escritura, migración, respaldo y
  administración.
- Revisa y elimina permisos que ya no sean necesarios.

En una arquitectura con varios servicios es preferible una base o esquema por
bounded context cuando el aislamiento lo requiera. Si se comparte un motor, la
separación de usuarios, esquemas y permisos debe seguir siendo explícita.

### Gestión de credenciales

- No hardcodees contraseñas en el código, imágenes, repositorios ni scripts.
- Almacena los secretos en un gestor dedicado y aplica control de acceso,
  auditoría y rotación.
- Usa credenciales distintas por entorno y por servicio.
- Define expiración, rotación y revocación antes de poner el sistema en
  producción.
- No incluyas contraseñas en mensajes de error, logs o tickets.
- Las migraciones deben usar una identidad administrativa separada de la que
  procesa solicitudes de usuarios.

## 2. Cifrado

### En tránsito

- Exige TLS entre la aplicación y el motor de base de datos, incluso dentro de
  una red privada.
- Verifica el certificado del servidor y evita modos que acepten conexiones sin
  validación.
- En PostgreSQL, un cliente puede requerir como mínimo `sslmode=require`; en
  producción suele ser preferible `verify-full` con la autoridad certificadora
  configurada. Usa el equivalente seguro del motor elegido.
- No confundas mTLS entre servicios con el cifrado específico de la conexión a
  la base de datos: son capas diferentes.
- Documenta versiones de TLS, autoridades certificadoras y procedimiento de
  rotación.

### En reposo

- Cifra discos, volúmenes, snapshots, réplicas y archivos temporales.
- Confirma que el proveedor realmente cifra todas las copias y no solo el
  almacenamiento principal.
- Protege las claves de cifrado con un servicio de gestión de claves y separa
  los permisos para usar una clave de los permisos para administrarla.

### Cifrado por columna

Para secretos de alto impacto, considera cifrado a nivel de aplicación o de
columna. La clave no debe almacenarse junto al dato cifrado.

Ejemplos de datos que pueden requerirlo:

- secretos de autenticación multifactor;
- tokens de proveedores;
- información financiera especialmente sensible;
- identificadores cuyo acceso debe quedar restringido incluso a lectores de la
  base de datos.

El cifrado por columna no reemplaza TLS, cifrado de disco, control de acceso ni
auditoría.

## 3. Aislamiento de red

- La base de datos no debe tener una dirección pública salvo una excepción
  justificada y protegida.
- Permite conexiones solo desde subredes, security groups, firewalls o rangos
  de red necesarios.
- Restringe cada puerto y protocolo; no abras la red completa por comodidad.
- Separa la red de administración de la red de tráfico de la aplicación.
- Usa bastiones o acceso temporal auditado para tareas administrativas.
- Deshabilita accesos directos desde estaciones de trabajo y desde redes de
  usuarios finales.

Una conexión local o una red privada reducen exposición, pero no sustituyen la
autenticación, autorización y cifrado.

## 4. Integridad y constraints del esquema

La base de datos debe rechazar estados imposibles aunque exista un error en la
aplicación.

Usa, según corresponda:

- `NOT NULL` para datos obligatorios;
- `CHECK` para rangos e invariantes;
- `UNIQUE` para evitar duplicados;
- claves primarias y foráneas;
- tipos adecuados para fechas, importes, identificadores y estados;
- reglas `ON DELETE` y `ON UPDATE` explícitas;
- índices únicos parciales cuando una regla aplique solo a algunas filas.

Ejemplo conceptual de una cuota:

```sql
CHECK (used_bytes >= 0 AND used_bytes <= quota_bytes)
```

Los constraints deben complementar transacciones, bloqueos y validación de
dominio. No deben usarse para ocultar una lógica de negocio incompleta.

## 5. Aislamiento por usuario o tenant

Cuando varias identidades comparten tablas, el filtro de propietario debe ser
obligatorio y no depender únicamente de la disciplina del código.

### Reglas de diseño

- La identidad se obtiene del contexto autenticado, no del cuerpo enviado por
  el cliente.
- Todo repositorio que lea o modifique datos de negocio recibe el propietario
  de forma obligatoria.
- Las consultas, índices, claves únicas y borrados deben considerar el
  propietario cuando corresponda.
- Las pruebas deben intentar leer y mutar datos pertenecientes a otra
  identidad.

### Row-Level Security

PostgreSQL y algunos motores equivalentes permiten políticas de aislamiento a
nivel de fila. Un patrón conceptual es:

```sql
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;

CREATE POLICY owner_isolation ON documents
  USING (owner_id = current_setting('app.owner_id')::uuid);
```

Antes de adoptar este patrón, define cómo se establece y limpia el contexto de
la conexión, cómo funcionan los pools y cómo se comportan las tareas
administrativas. Una conexión reutilizada con el contexto equivocado puede
romper el aislamiento.

RLS es una defensa adicional: no reemplaza autorización de aplicación,
separación de roles ni pruebas de aislamiento.

## 6. Auditoría y trazabilidad

La auditoría debe permitir responder quién accedió a qué, cuándo, desde dónde
y con qué resultado.

Registra, con la mínima cantidad de datos necesaria:

- cambios de permisos y roles;
- cambios de esquema y migraciones;
- lecturas y modificaciones sobre tablas críticas;
- exportaciones, restauraciones y operaciones administrativas;
- fallos de autenticación y violaciones de política;
- identificador de solicitud, servicio y resultado.

La auditoría de aplicación y la auditoría del motor son complementarias. Una
extensión o función de auditoría del motor puede registrar operaciones que el
código de aplicación olvide registrar, pero debe configurarse para no capturar
contraseñas, datos completos innecesarios o información que infrinja la
política de privacidad.

En PostgreSQL, `pgaudit` es una opción habitual para registrar accesos y cambios
seleccionados. Debe configurarse con una política de retención y filtrado que
evite registrar secretos o cantidades innecesarias de datos.

Protege los logs de auditoría contra modificación por el mismo rol que genera
los eventos y define retención, acceso y alertas.

## 7. Backups, snapshots y recuperación

- Cifra backups, snapshots, réplicas y exportaciones.
- Usa almacenamiento separado y acceso restringido.
- Mantén varias ventanas de retención y una copia aislada contra borrado o
  ransomware.
- No reutilices las credenciales de la aplicación para leer backups.
- Registra quién creó, descargó, restauró o eliminó una copia.
- Prueba restauraciones periódicas en un entorno aislado.
- Mide RPO y RTO reales; no los dejes como valores teóricos.
- Verifica integridad, consistencia, permisos y compatibilidad de la versión
  después de restaurar.
- Define cómo revocar claves si un backup o snapshot queda expuesto.

Un backup que nunca se restaura no es evidencia suficiente de recuperación.

## 8. Gestión de conexiones y recursos

- Define un límite de conexiones por servicio y por entorno.
- Usa pools con tamaños, timeouts de adquisición, timeouts de consulta y
  límites de vida configurados explícitamente.
- Reserva capacidad para migraciones, administración y recuperación.
- Evita que un servicio defectuoso pueda agotar las conexiones de todos los
  demás.
- Cancela consultas que excedan su presupuesto y controla consultas pesadas.
- Usa un proxy o pooler cuando la topología lo requiera, manteniendo la
  identidad y el aislamiento de cada servicio.
- Vigila conexiones activas, esperas, locks, transacciones abiertas y espacio
  temporal.

Los límites de recursos son una medida de disponibilidad y también de
seguridad: reducen el impacto de abuso, fugas de conexiones y errores de
concurrencia.

## 9. Consultas, transacciones y migraciones

- Usa consultas parametrizadas; nunca concatena entrada del usuario en SQL.
- Define timeouts y límites de filas para operaciones expuestas a usuarios.
- Mantén transacciones cortas y con un orden de locks consistente.
- Haz que operaciones críticas sean idempotentes cuando puedan repetirse.
- Versiona migraciones y revísalas antes de aplicarlas.
- Prueba migraciones hacia adelante y rollback o procedimiento de recuperación
  antes de una ventana crítica.
- No ejecutes cambios destructivos automáticos sin una copia recuperable y una
  aprobación explícita.
- Revisa planes de ejecución para evitar consultas que puedan convertirse en
  agotamiento de recursos.

## 10. Actualizaciones y parcheo

- Mantén el motor, extensiones, drivers, poolers y herramientas administrativas
  en versiones soportadas.
- Suscríbete a avisos de seguridad y define un SLA para aplicar parches.
- Prueba actualizaciones en un entorno representativo antes de producción.
- Documenta compatibilidad, cambios de parámetros y plan de reversión.
- Elimina extensiones, usuarios y módulos que ya no se utilicen.

El parcheo debe ser un proceso recurrente y medible, no una tarea ocasional.

## 11. Datos no productivos

- No copies datos reales a desarrollo o staging sin justificación y controles.
- Anonimiza o tokeniza correos, teléfonos, direcciones, datos financieros y
  secretos antes de exportarlos.
- Mantén claves, usuarios, redes y backups separados por entorno.
- Prueba que los scripts de seed no introduzcan datos reales.
- Elimina copias temporales después de la prueba y registra su eliminación.
- Aplica a los entornos no productivos la misma disciplina de acceso que a
  producción cuando contengan datos sensibles.

Un entorno de pruebas con datos reales es otra copia de producción y debe
tratarse como tal.

## 12. Roles de solo lectura

Para reportes, analítica o consultas operativas, crea roles que solo tengan
`SELECT` sobre las vistas o tablas necesarias.

- Prefiere vistas con columnas minimizadas a conceder acceso a tablas completas.
- Separa lectura de escritura y administración.
- Revisa periódicamente que el rol no haya recibido permisos adicionales.
- Evita que un servicio de reportes pueda acceder a secretos o modificar
  registros.

## 13. Monitoreo y alertas

Mide y alerta sobre:

- intentos de conexión rechazados;
- cambios de permisos, roles y políticas;
- consultas lentas y bloqueos prolongados;
- crecimiento anómalo de tablas, índices o almacenamiento;
- agotamiento de conexiones o espacio;
- fallos de backup y restauración;
- cambios de configuración y desactivación de auditoría;
- accesos desde redes, identidades o horarios inesperados.

Las métricas deben evitar PII innecesaria y los logs deben poder correlacionar
una operación con el servicio y la solicitud que la originó.

## 14. Pruebas de seguridad

Antes de producción y durante la operación:

- intenta conectar con credenciales incorrectas, expiradas y revocadas;
- verifica que cada rol solo pueda ejecutar sus operaciones autorizadas;
- intenta leer y modificar filas de otra identidad;
- prueba constraints, claves foráneas y carreras de concurrencia;
- comprueba que las consultas no acepten inyección SQL;
- simula pérdida de conexión, timeout, deadlock, réplica atrasada y falta de
  espacio;
- ejecuta una restauración real desde un backup cifrado;
- verifica que una rotación de credenciales no deje accesos antiguos activos;
- inspecciona auditoría y alertas después de cada escenario;
- ejecuta pruebas de carga con límites de conexiones y consultas costosas.

## 15. Lista de salida a producción

- [ ] Ningún servicio usa superusuario para el tráfico normal.
- [ ] Cada identidad tiene permisos mínimos y credenciales rotables.
- [ ] Las conexiones usan TLS con validación del servidor.
- [ ] El almacenamiento, snapshots y backups están cifrados.
- [ ] La base no está expuesta directamente a internet.
- [ ] Existen constraints para invariantes críticas.
- [ ] El aislamiento por propietario está aplicado y probado.
- [ ] La auditoría registra operaciones sensibles sin capturar secretos.
- [ ] Hay backups cifrados, retención definida y restauraciones verificadas.
- [ ] Los pools tienen límites, timeouts y capacidad reservada.
- [ ] Las consultas son parametrizadas y las migraciones están versionadas.
- [ ] El motor y sus extensiones reciben parches dentro del SLA definido.
- [ ] Los datos no productivos están anonimizados.
- [ ] Las alertas de acceso, capacidad, auditoría y backup están activas.
- [ ] Existe un procedimiento documentado de incidente, rotación y recuperación.

## 16. Principio final

La seguridad de la base de datos no depende de una sola configuración ni de que
cada desarrollador recuerde escribir el filtro correcto. Debe existir una
defensa en profundidad formada por identidades separadas, permisos mínimos,
cifrado, red restringida, constraints, aislamiento por fila, auditoría,
backups recuperables y pruebas continuas.
