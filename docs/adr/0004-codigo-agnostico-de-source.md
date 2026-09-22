# ADR-004 — Código agnóstico de source

- **Estado:** aceptado
- **Fecha:** 2026-09-22

## Contexto

QBZ nació como cliente de un solo servicio de streaming, y ese servicio se metió en todo: 14 archivos de la biblioteca, el player, los lyrics, la radio y las recomendaciones lo nombraban. Cuando el servicio salió, cortar ese acoplamiento fue la mitad del trabajo (auditoría CHD-0001).

En Chilpa, un servicio de streaming solo existe como plugin de terceros (ADR-003), fuera de este repo. El core no debe saber cuáles existen.

## Decisión

1. **Ningún servicio de streaming se nombra en este repo.** Esto aplica a código, identificadores, comentarios, tests, fixtures, documentación y mensajes de commit.
2. **El código habla de conceptos, no de marcas:** `source`, cuenta, proveedor, catálogo, disponibilidad y calidad. Cuando hace falta un ejemplo, se usa un source ficticio (`example-source`).
3. **El modelo usa descriptores genéricos.** Si un servicio tiene un concepto propio (un nivel de calidad, "comprado", "hi-res"), se traduce a descriptores comunes: formato, profundidad de bits, frecuencia de muestreo, canales, disponibilidad y derechos. Nunca se agrega un campo o un `enum` con el nombre de un servicio.
4. **Los servidores self-hosted con protocolo abierto sí se nombran** (Jellyfin, Subsonic/OpenSubsonic, Plex), porque son integraciones de primera parte y su crate implementa su protocolo. Aun así, fuera de su crate el resto del código solo ve `source`.
5. **Las vistas son agnósticas.** Una vista de álbum, de artista o de búsqueda junta el contenido de todos los sources y distingue su procedencia con los datos del source (nombre e ícono que declara el plugin), nunca con lógica propia para un servicio.

## Candados

- **La lista de nombres prohibidos** vive en `tools/lint/streaming-names.txt`. Es el único archivo del repo que puede contenerlos.
- **`tools/lint/no-platform-names`** busca esos nombres, como palabra completa y sin distinguir mayúsculas, en todos los archivos rastreados por git.
- **El hook de pre-commit** corre esa búsqueda, y el de `commit-msg` revisa el mensaje del commit. El CI correrá lo mismo cuando exista.
- Si un nombre legítimo choca con la lista, se discute y se ajusta la lista en el mismo commit que este ADR; nunca se agrega una excepción silenciosa.

## Consecuencias

- Agregar o quitar un servicio nunca toca el core.
- Un plugin de terceros no puede depender de un trato especial, porque no existe.
- A veces un ejemplo concreto sería más claro que uno ficticio. Se pierde un poco de claridad a cambio de que el código no vuelva a amarrarse a un servicio.
