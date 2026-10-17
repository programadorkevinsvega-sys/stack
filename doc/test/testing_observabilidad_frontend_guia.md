# Guía de Testing y Observabilidad para Frontend

Documento de referencia general — aplicable a cualquier proyecto de frontend (React/Next.js y equivalentes). Complementa la guía de backend con el mismo formato, pensado como checklist reutilizable para cualquier proyecto futuro.

---

## Cómo leer este documento

Cada punto incluye:
- **Qué es** — definición corta y concreta.
- **Para qué sirve** — el problema real que resuelve.
- **Por qué importa** — la consecuencia de no tenerlo.
- **Herramientas** — opciones concretas del ecosistema frontend actual.
- **Etapa de implementación** — cuándo conviene introducirlo.

Al final hay una **tabla de priorización global por etapa**, igual que en la guía de backend.

---

# PARTE 1 — TESTING

## 1.1 Pruebas unitarias

**Qué es:** pruebas de una función pura o pieza de lógica aislada (validadores, formateadores, cálculos, utilidades), sin renderizar componentes ni tocar red.

**Para qué sirve:** verificar que la lógica de negocio en el cliente (validación de formularios, formateo de moneda/fechas, cálculos de UI) es correcta en aislamiento.

**Por qué importa:** es la base más rápida y barata; sin esto, errores de lógica simple (una regex mal escrita, un cálculo de precio erróneo) solo se detectan manualmente o en producción.

**Herramientas:** `Vitest` (recomendado en proyectos modernos con Vite/Next.js) o `Jest`.

**Etapa de implementación:** **Día 0.** Se escribe junto con cualquier función de lógica de negocio en el cliente.

---

## 1.2 Component testing

**Qué es:** pruebas que renderizan un componente de forma aislada y verifican su comportamiento desde la perspectiva del usuario (qué se ve, qué pasa al hacer clic, qué aparece tras un cambio de estado).

**Para qué sirve:** confirmar que un componente se comporta correctamente sin necesidad de levantar toda la aplicación.

**Por qué importa:** detecta regresiones de UI/interacción de forma rápida, mucho antes de llegar a pruebas end-to-end costosas.

**Herramientas:** `React Testing Library` (con Vitest o Jest), `Vue Testing Library` si aplica.

**Etapa de implementación:** **Desarrollo temprano**, apenas existan componentes con lógica de interacción (formularios, modales, tablas con estado).

---

## 1.3 Pruebas de integración (frontend)

**Qué es:** pruebas que verifican varios componentes trabajando juntos, incluyendo la capa de llamadas a API, usando una API simulada que responde con contratos realistas.

**Para qué sirve:** confirmar flujos completos dentro del cliente (ej. "el formulario de login llama al endpoint correcto, maneja el 401 mostrando el campo de MFA, y redirige tras éxito") sin depender de que el backend real esté disponible.

**Por qué importa:** sin esto, la única forma de probar un flujo completo es contra el backend real, lo cual es lento, frágil y depende de que ese entorno esté arriba y con datos consistentes.

**Herramientas:** `MSW` (Mock Service Worker) para interceptar llamadas de red con contratos JSON reales, combinado con React Testing Library.

**Etapa de implementación:** **Desarrollo temprano**, en cuanto exista el primer flujo que combine varios componentes y llamadas a API (login, checkout, cualquier wizard multi-paso).

---

## 1.4 Pruebas end-to-end (E2E)

**Qué es:** pruebas que automatizan un navegador real (o headless) navegando la aplicación de principio a fin, tal como lo haría un usuario.

**Para qué sirve:** validar flujos críticos completos de extremo a extremo — registro, login, checkout, cualquier recorrido que cruce múltiples páginas y dependa de la integración real con el backend (o un entorno de staging).

**Por qué importa:** es la única capa de testing que realmente confirma "esto funciona como lo experimentaría un usuario", detectando problemas de integración real que ningún mock puede anticipar.

**Herramientas:** `Playwright` (recomendado, mejor soporte multi-navegador y más rápido), `Cypress`.

**Etapa de implementación:** **Antes de producción**, cubriendo como mínimo los flujos de negocio críticos (registro→verificación→login, checkout/pago, la acción principal del producto).

---

## 1.5 Visual regression testing

**Qué es:** captura screenshots de componentes/páginas y los compara automáticamente contra una versión de referencia, marcando cualquier diferencia visual no intencional.

**Para qué sirve:** detectar roturas visuales que ningún test funcional detecta (un botón que se desborda, un color que cambió sin querer, un layout roto en un breakpoint específico).

**Por qué importa:** los tests funcionales confirman que "el código hace lo correcto", pero no que "se ve correcto" — cambios de CSS/estilos rotos son un punto ciego común sin esto.

**Herramientas:** `Chromatic` (integrado con Storybook), `Percy`, o capturas de pantalla nativas de `Playwright` con comparación de diff.

**Etapa de implementación:** **Antes de producción**, especialmente si el proyecto usa un sistema de diseño/componentes reutilizables donde una rotura se propaga a muchas pantallas.

---

## 1.6 Pruebas de accesibilidad (a11y)

**Qué es:** verificación automatizada y manual de que la interfaz es usable con lectores de pantalla, navegación por teclado, contraste de color adecuado y semántica HTML correcta.

**Para qué sirve:** asegurar que la aplicación es utilizable por personas con discapacidades visuales, motoras o cognitivas.

**Por qué importa:** más allá de la obligación ética, en muchas jurisdicciones es un requisito legal (ADA en EE.UU., normativas equivalentes en la UE/LatAm); además, buena accesibilidad casi siempre mejora la UX general (mejor semántica, mejor navegación por teclado, mejor SEO).

**Herramientas:** `axe-core` (integrable en tests automatizados), auditoría de accesibilidad de `Lighthouse`, pruebas manuales con lector de pantalla (VoiceOver/NVDA) en flujos críticos.

**Etapa de implementación:** **Desarrollo temprano**, integrando `axe-core` en los component tests desde el principio — corregir accesibilidad después de construir cientos de componentes es mucho más costoso que hacerlo desde el diseño.

---

## 1.7 Contract testing (frontend-backend)

**Qué es:** verifica que los contratos de API que el frontend espera coincidan exactamente con lo que el backend realmente expone, sin necesitar ambos entornos corriendo juntos.

**Para qué sirve:** detectar que un cambio en el backend (un campo renombrado, un tipo cambiado) rompe al frontend antes del despliegue, no después.

**Por qué importa:** en proyectos con backend y frontend desarrollados por separado (o por equipos distintos), esta es la principal fuente de bugs de integración que solo aparecen en producción.

**Herramientas:** `Pact` (consumer-driven contracts), generación de tipos TypeScript directamente desde el esquema OpenAPI/Protobuf del backend para que un cambio de contrato rompa la compilación en vez de fallar en tiempo de ejecución.

**Etapa de implementación:** **Antes de producción**, y de forma continua si backend y frontend evolucionan en paralelo con cierta independencia.

---

## 1.8 Snapshot testing

**Qué es:** congela la salida renderizada de un componente y detecta cualquier cambio no intencional frente a esa referencia.

**Para qué sirve:** detectar cambios accidentales en la estructura de componentes al refactorizar.

**Por qué importa:** sin disciplina, los snapshots pueden volverse ruido (la gente los actualiza sin revisar); usados con criterio, protegen estructuras críticas de cambios silenciosos.

**Herramientas:** snapshot testing nativo de `Vitest`/`Jest`.

**Etapa de implementación:** **Desarrollo temprano**, con uso selectivo — no abusar de snapshots grandes de páginas completas, mejor en componentes puntuales con salida predecible.

---

## 1.9 Property-based testing (frontend)

**Qué es:** igual que en backend, genera entradas aleatorias y verifica que una propiedad/invariante siempre se cumpla — aplicado aquí a validadores de formularios, parsers de input del usuario, lógica de formateo.

**Para qué sirve:** encontrar casos límite en validación de datos que los tests manuales no cubrirían (ej. "el validador de email nunca debe aceptar un string sin arroba, sin importar qué caracteres raros se agreguen alrededor").

**Por qué importa:** los formularios son la superficie de mayor exposición a input impredecible del usuario; los casos raros ahí generan bugs de validación reales.

**Herramientas:** `fast-check`.

**Etapa de implementación:** **Desarrollo temprano**, en la lógica de validación de formularios más crítica (contraseñas, montos de pago, datos de facturación).

---

## 1.10 Mutation testing (frontend)

**Qué es:** igual que en backend — modifica automáticamente el código y verifica si los tests existentes detectan el cambio.

**Para qué sirve:** confirmar que la suite de tests de componentes/lógica realmente detecta errores, no solo que "pasa".

**Por qué importa:** una cobertura alta de tests superficiales da una falsa sensación de seguridad.

**Herramientas:** `Stryker Mutator` (soporta JS/TS).

**Etapa de implementación:** **Fase de madurez**, cuando ya existe una suite de tests considerable que vale la pena auditar.

---

## 1.11 Performance testing / presupuestos de rendimiento

**Qué es:** medición automatizada de métricas de rendimiento de carga (tiempo de carga, tamaño de bundle, Core Web Vitals) contra un umbral definido ("presupuesto"), fallando el build si se excede.

**Para qué sirve:** evitar que el rendimiento se degrade gradualmente a medida que se agregan features y dependencias, sin que nadie lo note hasta que el producto se siente "lento".

**Por qué importa:** el rendimiento frontend impacta directamente conversión, retención y (en el caso de Core Web Vitals) posicionamiento SEO — degradarlo silenciosamente tiene costo de negocio real.

**Herramientas:** `Lighthouse CI` (corre auditorías automáticas en cada PR), `size-limit` o `bundlephobia` (presupuesto de tamaño de bundle JS).

**Etapa de implementación:** **Antes de producción**, estableciendo el presupuesto inicial cuando el sitio aún es liviano — es mucho más fácil mantener un presupuesto que "arreglar" un bundle que ya creció sin control.

---

## 1.12 Pruebas cross-browser / cross-device

**Qué es:** ejecutar los tests (especialmente E2E y visual) contra múltiples navegadores (Chrome, Firefox, Safari) y tamaños de pantalla (desktop, tablet, móvil).

**Para qué sirve:** detectar incompatibilidades específicas de un motor de navegador o un breakpoint responsivo que no aparecen probando solo en Chrome desktop.

**Por qué importa:** Safari/WebKit en particular tiene comportamientos distintos a Chrome en varias APIs web; sin probarlo explícitamente, esos bugs solo se descubren cuando un usuario real los reporta.

**Herramientas:** `Playwright` (soporta multi-navegador nativamente), `BrowserStack`/`Sauce Labs` para matrices más amplias de dispositivos reales.

**Etapa de implementación:** **Antes de producción**, cubriendo como mínimo Chrome, Safari y un viewport móvil en los flujos críticos.

---

## 1.13 Security testing (frontend)

**Qué es:** auditoría de dependencias del proyecto (paquetes npm con vulnerabilidades conocidas) y verificación de protecciones contra ataques comunes del lado del cliente (XSS, inyección de contenido, política de seguridad de contenido).

**Para qué sirve:** prevenir que una dependencia vulnerable o una mala práctica (ej. `dangerouslySetInnerHTML` sin sanitizar) exponga a los usuarios a ataques.

**Por qué importa:** el frontend es la superficie más expuesta directamente a los usuarios finales; un XSS explotado puede robar sesiones o datos sensibles directamente del navegador de la víctima.

**Herramientas:** `npm audit` / `pnpm audit`, `Snyk`, verificación de cabeceras de Content Security Policy (CSP), sanitización explícita de cualquier HTML dinámico (`DOMPurify`).

**Etapa de implementación:** **Día 0** para la auditoría de dependencias en CI (barato); CSP y sanitización de HTML dinámico **antes de producción**.

---

## 1.14 Load testing de assets/frontend

**Qué es:** pruebas de carga enfocadas en la entrega de assets estáticos (JS, CSS, imágenes) y en cómo se comporta la aplicación bajo muchas sesiones de navegador simultáneas, no solo el backend.

**Para qué sirve:** confirmar que el CDN/hosting de assets estáticos y cualquier renderizado del lado del servidor (SSR) soportan el tráfico esperado sin degradarse.

**Por qué importa:** con Next.js u otros frameworks SSR, el servidor de renderizado puede ser un cuello de botella independiente del backend de API.

**Herramientas:** `k6` con su módulo de navegador (`k6 browser`), pruebas de carga dirigidas específicamente a las rutas SSR más pesadas.

**Etapa de implementación:** **Antes de producción**, si el proyecto usa SSR; menor prioridad si es un SPA puramente estático servido por CDN.

---

## 1.15 Smoke tests post-deploy

**Qué es:** igual que en backend — un set rápido de verificaciones tras cada despliegue confirmando que las rutas y flujos esenciales cargan correctamente.

**Para qué sirve:** detectar de inmediato si un despliegue de frontend rompió algo crítico (una página en blanco, un error de build no detectado, un asset faltante).

**Por qué importa:** los errores de despliegue de frontend (rutas 404, assets con hash incorrecto, variables de entorno faltantes en build) son comunes y muchas veces silenciosos hasta que un usuario reporta una pantalla rota.

**Herramientas:** un subconjunto pequeño de los mismos tests E2E de Playwright, ejecutado automáticamente tras cada despliegue.

**Etapa de implementación:** **Antes del primer despliegue a producción real.**

---

# PARTE 2 — OBSERVABILIDAD

## 2.1 Real User Monitoring (RUM)

**Qué es:** captura de métricas de rendimiento reales desde el navegador de usuarios reales (no sintéticas) — Core Web Vitals (LCP, INP, CLS), tiempos de carga, errores de red, todo medido en producción con tráfico real.

**Para qué sirve:** saber cómo experimentan realmente el rendimiento los usuarios, con toda la variedad de dispositivos, redes y navegadores reales — algo que ningún test de laboratorio replica del todo.

**Por qué importa:** un sitio que es rápido en la máquina del desarrollador con buena conexión puede ser lento para un usuario real en un celular de gama media con red 4G — sin RUM, esa brecha es invisible.

**Herramientas:** `Vercel Analytics` / `Vercel Speed Insights` (si se usa Next.js en Vercel), `Google CrUX` / `PageSpeed Insights`, `Sentry Performance`, `Datadog RUM`.

**Etapa de implementación:** **Antes de producción**, integrado desde el primer despliegue real para tener datos históricos desde el día uno.

---

## 2.2 Error tracking

**Qué es:** captura automática de errores de JavaScript no controlados en el navegador del usuario (excepciones, promesas rechazadas, errores de render de React), con stack trace, contexto del usuario y frecuencia.

**Para qué sirve:** enterarse de errores reales que ocurren en producción sin depender de que el usuario los reporte manualmente (la gran mayoría nunca lo hace).

**Por qué importa:** es la ventana más directa a "qué se está rompiendo de verdad para usuarios reales ahora mismo" — sin esto, los bugs de producción quedan invisibles hasta que alguien se queja lo suficientemente fuerte.

**Herramientas:** `Sentry` (el más usado y con mejor integración con frontend moderno), `Bugsnag`.

**Etapa de implementación:** **Antes de producción**, configurado desde el primer despliegue.

---

## 2.3 Session replay

**Qué es:** grabación (generalmente anonimizada) de las interacciones reales del usuario en la sesión donde ocurrió un error o un comportamiento anómalo, para poder "ver" exactamente qué hizo.

**Para qué sirve:** reproducir bugs difíciles de describir solo con un stack trace — ver los clics, el scroll, el estado de la UI exacto en el momento del fallo.

**Por qué importa:** muchos bugs de frontend dependen de una secuencia específica de interacciones difícil de adivinar solo con logs; ver la sesión real ahorra horas de tratar de reproducir el problema a ciegas.

**Herramientas:** `Sentry Session Replay` (integrado con el error tracking), `LogRocket`, `Hotjar` (más orientado a UX/producto que a debugging).

**Etapa de implementación:** **Producción temprana**, una vez que exista tráfico real y los primeros errores empiecen a reportarse sin contexto suficiente.

**Nota de privacidad:** enmascarar explícitamente campos sensibles (contraseñas, datos de tarjeta, información personal) antes de habilitar esto — la mayoría de herramientas lo permiten configurar, pero no viene seguro por defecto.

---

## 2.4 Logging estructurado del cliente

**Qué es:** registro estructurado (no solo `console.log`) de eventos relevantes del lado del cliente, enviados a un backend de logs centralizado.

**Para qué sirve:** correlacionar comportamiento del cliente con lo que pasa en el backend (ej. adjuntar el mismo `trace_id` que usa la observabilidad del backend a las llamadas de API del frontend).

**Por qué importa:** sin esto, depurar un problema que cruza frontend y backend obliga a revisar dos sistemas de logs desconectados sin forma de correlacionarlos.

**Herramientas:** envío de logs estructurados hacia el mismo stack de observabilidad del backend (ej. Loki), propagando el `trace_id` de OpenTelemetry desde el cliente.

**Etapa de implementación:** **Antes de producción**, si el backend ya tiene trazas distribuidas (ver guía de backend) — el mayor valor de esto es la correlación end-to-end.

---

## 2.5 Synthetic monitoring / uptime checks

**Qué es:** igual que en backend — un proceso externo que visita periódicamente la aplicación simulando a un usuario, sin depender de tráfico real.

**Para qué sirve:** detectar que el sitio está caído o roto (una página en blanco, un error de build desplegado) incluso con cero usuarios activos.

**Por qué importa:** un frontend caído de madrugada sin usuarios conectados pasa desapercibido hasta la mañana sin esto.

**Herramientas:** `Lighthouse CI` en modo programado, `Grafana Synthetic Monitoring`, `Checkly` (especializado en checks sintéticos de frontend/API).

**Etapa de implementación:** **Antes de producción.**

---

## 2.6 Analytics de producto

**Qué es:** seguimiento de eventos de uso real del producto (qué páginas se visitan, qué botones se usan, dónde abandonan los usuarios un flujo).

**Para qué sirve:** entender el comportamiento real de los usuarios para tomar decisiones de producto informadas, no solo detectar errores técnicos.

**Por qué importa:** complementa el error tracking — un flujo puede "funcionar técnicamente sin errores" y aun así tener una tasa de abandono altísima por mala UX, algo que solo el analytics revela.

**Herramientas:** `PostHog` (open source, incluye feature flags y session replay también), `Amplitude`, `Google Analytics`/`GA4`.

**Etapa de implementación:** **Antes de producción**, definiendo los eventos clave del funnel de negocio (registro, activación, conversión) desde el lanzamiento.

---

## 2.7 Feature flags con observabilidad de rollout

**Qué es:** sistema para activar/desactivar features de forma gradual (por porcentaje de usuarios, por plan, por segmento), con métricas de qué grupo está viendo qué versión.

**Para qué sirve:** desplegar cambios riesgosos de forma controlada, pudiendo revertir instantáneamente sin un nuevo despliegue si algo sale mal.

**Por qué importa:** reduce drásticamente el radio de impacto de un error en producción — en vez de que un bug afecte al 100% de usuarios de inmediato, afecta solo al pequeño porcentaje del rollout inicial.

**Herramientas:** `LaunchDarkly`, `Unleash` (open source), `PostHog Feature Flags`.

**Etapa de implementación:** **Producción temprana**, en cuanto el ritmo de despliegues empiece a incluir cambios de mayor riesgo (nuevos flujos de pago, rediseños grandes).

---

## 2.8 Presupuestos de rendimiento como SLO (Core Web Vitals)

**Qué es:** definir formalmente un umbral aceptable para las métricas de Core Web Vitals (ej. "LCP < 2.5s para el 75% de las cargas reales") y alertar cuando se degrada, igual que un SLO de backend pero para rendimiento percibido del cliente.

**Para qué sirve:** darle un objetivo medible y accionable al rendimiento frontend, en vez de que sea una preocupación vaga ("se siente lento").

**Por qué importa:** conecta directamente con el RUM (2.1) — sin un umbral definido, los datos de rendimiento real se acumulan sin que nadie decida cuándo actuar.

**Herramientas:** paneles de `Vercel Speed Insights` o `Datadog RUM` con alertas configuradas sobre el percentil 75 de Core Web Vitals.

**Etapa de implementación:** **Producción temprana**, una vez que exista suficiente tráfico real para que el percentil sea estadísticamente significativo.

---

## 2.9 Alertas de picos de errores en cliente

**Qué es:** alertas automáticas cuando la tasa de errores capturados por el error tracking sube anormalmente respecto a su línea base.

**Para qué sirve:** detectar que un despliegue reciente introdujo una regresión, sin esperar a revisar el dashboard manualmente.

**Por qué importa:** conecta directamente con el error tracking (2.2) — capturar errores sin alertar sobre picos anómalos significa que alguien tiene que estar mirando el dashboard activamente para notar un problema.

**Herramientas:** alertas nativas de `Sentry` sobre tasa de errores por release/versión desplegada.

**Etapa de implementación:** **Junto con el error tracking (2.2)**, configurado desde el primer despliegue.

---

## 2.10 Observabilidad de experimentos (A/B testing)

**Qué es:** métricas específicas para comparar el comportamiento/conversión entre variantes de un experimento controlado.

**Para qué sirve:** validar con datos si un cambio de UI/flujo realmente mejora una métrica de negocio, en vez de asumirlo.

**Por qué importa:** decisiones de producto basadas en intuición sin medición pueden empeorar métricas clave sin que nadie lo note hasta mucho después.

**Herramientas:** `PostHog Experiments`, `GrowthBook` (open source), o el módulo de experimentación de la herramienta de feature flags ya elegida.

**Etapa de implementación:** **Fase de madurez**, cuando el producto ya tiene tráfico suficiente para que un experimento controlado tenga significancia estadística.

---

## 2.11 Monitoreo de tamaño de bundle y dependencias

**Qué es:** seguimiento continuo (no solo en CI, sino como tendencia histórica) del tamaño del bundle de JavaScript y de las dependencias que lo componen.

**Para qué sirve:** detectar crecimiento gradual e inadvertido del peso de la aplicación a medida que se agregan librerías.

**Por qué importa:** el "bloat" de bundle es acumulativo y casi nunca se nota en un solo PR — se descubre meses después como un problema grande y costoso de revertir.

**Herramientas:** `size-limit` (con tendencia histórica en CI), `bundlephobia` para evaluar el costo de una dependencia antes de agregarla.

**Etapa de implementación:** **Antes de producción**, estableciendo el presupuesto inicial (ver 1.11) y revisándolo en cada PR que agregue dependencias nuevas.

---

## 2.12 Runbooks vinculados a alertas de frontend

**Qué es:** igual que en la guía de backend — cada alerta (pico de errores, degradación de Core Web Vitals, caída de uptime) vinculada a un documento corto con los primeros pasos a seguir.

**Para qué sirve:** que cualquier persona del equipo pueda reaccionar consistentemente sin depender de conocimiento tribal.

**Por qué importa:** sin esto, cada incidente de frontend se investiga desde cero, alargando el tiempo de resolución.

**Herramientas:** documentación versionada enlazada desde cada alerta configurada.

**Etapa de implementación:** **Junto con cada alerta creada**, desde el principio.

---

# PARTE 3 — ROADMAP DE PRIORIZACIÓN POR ETAPA

## Etapa 0 — Día 0 / Fundación del proyecto

| Punto | Sección |
|---|---|
| Pruebas unitarias | 1.1 |
| Auditoría de dependencias (`npm audit`) en CI | 1.13 |

## Etapa 1 — Desarrollo temprano (primeras features funcionando)

| Punto | Sección |
|---|---|
| Component testing | 1.2 |
| Pruebas de integración con MSW | 1.3 |
| Accesibilidad (`axe-core` integrado a component tests) | 1.6 |
| Snapshot testing (uso selectivo) | 1.8 |
| Property-based testing en validadores de formularios | 1.9 |

## Etapa 2 — Antes de producción / antes del primer cliente de pago

| Punto | Sección |
|---|---|
| Pruebas E2E de flujos críticos | 1.4 |
| Visual regression testing | 1.5 |
| Contract testing frontend-backend | 1.7 |
| Presupuestos de rendimiento (Lighthouse CI, `size-limit`) | 1.11 |
| Cross-browser / cross-device testing | 1.12 |
| CSP y sanitización de HTML dinámico | 1.13 |
| Load testing de SSR/assets (si aplica) | 1.14 |
| Smoke tests post-deploy | 1.15 |
| RUM (Core Web Vitals reales) | 2.1 |
| Error tracking | 2.2 |
| Logging estructurado correlacionado con backend | 2.4 |
| Synthetic monitoring | 2.5 |
| Analytics de producto | 2.6 |
| Monitoreo de bundle/dependencias | 2.11 |
| Alertas de picos de error | 2.9 |
| Runbooks por cada alerta | 2.12 |

## Etapa 3 — Producción temprana con usuarios reales

| Punto | Sección |
|---|---|
| Session replay (con enmascaramiento de datos sensibles) | 2.3 |
| Feature flags con observabilidad de rollout | 2.7 |
| Presupuestos de rendimiento como SLO (Core Web Vitals) | 2.8 |

## Etapa 4 — Fase de madurez / escala

| Punto | Sección |
|---|---|
| Mutation testing | 1.10 |
| Observabilidad de experimentos A/B | 2.10 |

---

## Principio general para cualquier proyecto de frontend futuro

1. **Lo que protege contra errores silenciosos en producción (error tracking, RUM, presupuestos de rendimiento) se instala antes del primer usuario real**, no después del primer incidente.
2. **Accesibilidad y contract testing se vuelven exponencialmente más caros de corregir cuanto más tarde se introducen** — conviene tratarlos como requisito desde el diseño de componentes, no como auditoría posterior.
3. **Session replay, experimentación y feature flags avanzados requieren tráfico real para aportar valor** — no tiene sentido invertir tiempo en ellos antes de tener usuarios.
4. **Testing de frontend y observabilidad de frontend deben correlacionarse con las del backend** (mismo `trace_id`, mismo stack de logs cuando sea posible) — un incidente real casi siempre cruza ambas capas, y diagnosticarlo por separado cuesta tiempo innecesario.