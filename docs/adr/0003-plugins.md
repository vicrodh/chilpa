# ADR-003 — Plugins

- **Estado:** aceptado
- **Fecha:** 2026-09-22

## Contexto

Chilpa tiene plugins desde el día cero. Un tercero debe poder usar el layout, el player y la cola, e incluso conectar una fuente de playback completa (un servicio de streaming, por ejemplo). Nosotros solo documentamos cómo. Hay dos condiciones que no se negocian:

1. Un plugin nunca ve tokens, contraseñas ni secretos de Chilpa o de otros plugins.
2. Un plugin roto nunca rompe Chilpa. Antes de romperla, no carga.

## Decisión

### Dos runtimes, una sola API

- **WASM**, para la lógica: sources, metadata, scrobblers, reglas de smart lists y proveedores de lyrics. Corre en sandbox, dentro del proceso, con límites de memoria y de tiempo por llamada.
- **Proceso aparte**, para lo que necesite código nativo (por ejemplo, el SDK de un servicio). Chilpa lanza el proceso y habla con él por IPC.
- Los dos exponen **la misma API versionada** (semver). Un plugin declara con qué versión de la API trabaja, y Chilpa rechaza cargar una versión incompatible.
- El motor WASM concreto y el formato del IPC se deciden en el spike de plugins. Cualquier dependencia tiene que ser compatible con GPL-2.0-only.

### Manifest y permisos

Cada plugin trae un manifest con `id`, versión, versión de la API, tipo, runtime y los permisos que pide. El usuario los aprueba al instalarlo. Los permisos posibles son:

- red solo a los dominios listados;
- lectura y escritura en su propia carpeta;
- lectura de la biblioteca;
- registrar una source, un panel o un comando.

Un plugin tiene su propio almacén de secretos, por ejemplo para guardar el token de su servicio. Nunca puede leer el de Chilpa ni el de otro plugin.

### Playback: el core es dueño del audio

Un plugin de source entrega una URL, un stream o bytes codificados. Chilpa los decodifica y los manda por su propia ruta de salida. Ningún plugin toca el dispositivo de audio, el hilo de tiempo real ni la configuración bit-perfect. Así, el audio de un plugin es tan bit-perfect como el de un archivo local.

### UI: descriptores, no código Qt

Un plugin de UI entrega datos más un descriptor de vista, armado con un conjunto limitado de componentes (lista, grid, detalle, acciones). El shell lo dibuja con los mismos componentes que las vistas propias. En la v1 no se permite QML ni código Qt arbitrario dentro del proceso, porque eso rompería el aislamiento y el ADR-001.

### Watchdog

- **Límites:** memoria máxima y tiempo máximo por llamada en los dos runtimes. En WASM se usa el control de consumo del motor. En proceso aparte, límites del sistema operativo, timeouts y un heartbeat.
- **Si una llamada rebasa su límite o el plugin truena,** el plugin se detiene, Chilpa sigue y el usuario recibe un aviso.
- **Si truena al cargar,** no carga. Chilpa arranca igual.
- **Tres fallas en una ventana corta** mandan el plugin a cuarentena. Solo el usuario lo reactiva.
- **Las llamadas a plugins nunca corren en el hilo de audio.** Todas son asíncronas y tienen timeout.

### Las features propias son plugins

last.fm, MusicBrainz, la radio, los podcasts y los proveedores de lyrics se construyen como plugins de primera parte, con la misma API que los de terceros. Si la API no aguanta nuestras propias features, no sirve.

### Licencia: excepción de plugins

Chilpa es GPL-2.0-only con una excepción: un plugin que se comunica con Chilpa **solo** a través de la API pública de plugins puede tener cualquier licencia. El texto está en [LICENSE-EXCEPTION-PLUGINS](../../LICENSE-EXCEPTION-PLUGINS). Sin ella, un plugin de terceros podría considerarse obra derivada y quedar obligado a ser GPL.

## Candados

- **Suite de plugins hostiles en CI.** Son plugins de prueba que:
    - hacen panic;
    - se quedan en un loop infinito;
    - se comen la memoria;
    - intentan leer secretos ajenos;
    - salen a un dominio no declarado;
    - declaran una versión de API incompatible.

  Chilpa tiene que seguir funcionando y bloquear cada caso.
- **Pruebas de compatibilidad de la API.** Un cambio que rompa la API sin subir la versión mayor falla.

## Consecuencias

- Un tercero puede hacer un plugin de playback para cualquier servicio, con la licencia que quiera, sin tocar el core.
- Escribir un plugin cuesta más que cargar una `.so`, a cambio de que ninguno pueda tumbar Chilpa ni robar secretos.
- La UI de plugins es menos libre que la de una vista propia. Si un descriptor se queda corto, se amplía el conjunto de componentes.

## Pendiente

- [ ] Motor WASM y formato del IPC (spike de plugins).
- [ ] Superficie de la API v0.
- [ ] Revisar el texto de la excepción con alguien que sepa de licencias antes de hacer público el proyecto.
