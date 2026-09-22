# ADR-001 — Sin frontend monolítico

- **Estado:** propuesto
- **Fecha:** 2026-09-21

## Contexto

En QBZ la regla "no hagamos un frontend monolítico" existía y se rompió igual. Al cierre de 2.1.2, `qbz-qt` tenía ~165k LOC y dependía directo de 38 crates del workspace, incluidos los internos del player, la biblioteca y Qobuz. Tres agentes distintos (Claude, Kimi, Codex) la ignoraron, porque la regla solo vivía en un documento y nada fallaba al romperla.

**Lección:** una regla de arquitectura sin candado automático no es una regla.

## Decisión

1. **El core es headless.** Toda la lógica vive en crates sin tipos de Qt. El core expone una sola API pública (comandos, eventos y view-models) en un crate de fachada, `chilpa-api` (nombre por confirmar).
2. **Un frontend solo depende de la fachada.** Un shell (Widgets o Quick) puede depender de `chilpa-api` y de sus propios crates de UI. Nunca de crates internos: player, biblioteca, sources, audio, etc.
3. **La UI se parte por vista.** Cada vista o panel es su propio módulo o crate, con su propio view-model. El shell solo compone.
4. **Los plugins usan la misma API.** Si una vista propia necesita algo que la API no da, se amplía la API; no hay puerta trasera.

## Candados (CI falla si se rompen)

| Candado | Cómo |
| --- | --- |
| Grafo de dependencias | Un chequeo sobre `cargo metadata` compara cada arista contra una lista permitida. Un frontend que dependa de un crate interno rompe el build. |
| Core sin Qt | Si aparece `cxx_qt`, `cxx-qt-lib` o `QString` fuera de los crates de shell, es error. |
| Tamaño por crate de UI | Límite de LOC por crate de UI. Pasarlo rompe el build y obliga a partir la vista. |
| Tamaño por archivo | Límite de LOC por archivo en todo el workspace, con excepciones explícitas y justificadas en la lista. |

Los números de los límites se deciden al crear el workspace. Cambiar la lista permitida o los límites requiere editar este ADR en el mismo commit.

## Consecuencias

- Cambiar de Widgets a Quick, o tener los dos, cuesta un shell nuevo, no una reescritura.
- Agregar una feature de UI a veces obliga a ampliar la API primero. Es más lento al inicio, y es justo lo que evita el monolito.
- Para los agentes: si un cambio necesita romper un candado, se detienen y preguntan. Nunca se relaja el candado para que pase.

## Pendiente

- [ ] Nombre final de la fachada.
- [ ] Límite de LOC por crate de UI y por archivo.
