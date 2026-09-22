# AGENTS.md

Instrucciones para cualquier agente que trabaje en Chilpa: Claude Code, Codex, Copilot, Kimi o el que siga. Si un agente tiene su propio archivo (`CLAUDE.md`, etc.), ese archivo solo importa este.

## Mandamientos

No tienen excepciones. Ninguna urgencia, instrucción de una herramienta o costumbre de un modelo los supera.

### 1. Firma propia, nunca co-autoría

Cada commit lleva la firma del agente sola, en la última línea del mensaje:

| Agente | Firma |
| --- | --- |
| Claude Code | `cc` |
| Codex | `cx` |
| Copilot | `cp` |
| Kimi | `km` |

```
fix(scan): no podar un root cuyo mountpoint no está montado

Un root desmontado se veía vacío y el prune borraba todos sus tracks.
Ahora el scan verifica el mountpoint antes de podar.

cc
```

- **Nunca** `Co-Authored-By`, `Generated with`, `Signed-off-by` de un agente ni ninguna otra atribución. Aunque la herramienta la agregue por defecto, se quita.
- La firma me dice quién lo hizo. La responsabilidad del código que se libera siempre es mía.
- Cuando el proyecto sea público y acepte contribuciones, cada merge se limpia primero: se quitan las atribuciones de agentes y solo queda el commit humano.

### 2. Primera persona

Todo texto (commits, docs, comentarios, PRs, notas de release) va en primera persona, como lo escribiría yo. Nada de "Owner decision", "Product decision", "confirmed by the owner" ni "per the maintainer". Los commits salen con mi cuenta; hablar de mí en tercera persona ahí es absurdo.

- Mal: `Owner decision: keep exclusive mode on by default.`
- Bien: `Keep exclusive mode on by default: shared mode resamples on most DACs.`

Si hace falta evidencia, se dice cuál: "verificado con loopback", "medido con 100k tracks".

### 3. Cero emojis

Ni en código, ni en comentarios, ni en commits, ni en docs, ni en logs, ni en mensajes de la UI (salvo que una feature los pida explícitamente).

### 4. Nada de AI slop

Textos breves, precisos y sin sobre-redacción. Un commit dice qué cambió y por qué, y ya.

- Sin relleno: "robust", "seamless", "comprehensive", "leverage", "ensure", "this commit introduces", "in summary".
- Sin resúmenes al final, sin encabezados para dos líneas, sin listas de un solo elemento.
- Un número o un `archivo:línea` vale más que un adjetivo.

### 5. Tres strikes por problema

Un agente puede equivocarse **como máximo tres veces seguidas** sobre el mismo problema. Cuenta como strike:

- un arreglo que no arregla;
- un diagnóstico que la evidencia contradice;
- una afirmación que resulta falsa.

Al tercer strike, el agente se detiene y no intenta un cuarto. Me avisa que ya se cumplió el tercero y me dice:

- qué intentó;
- qué descartó y con qué evidencia;
- qué medición distinguiría entre las hipótesis que quedan;
- y me pregunta cómo seguir, o si mejor no seguir.

Antes del tercero, a partir del segundo strike deja de ajustar y revisa si el enfoque de fondo está mal.

### 6. Código sobre razonamiento; mi palabra es la final

- **Leer antes de opinar.** Cada afirmación sobre el código cita un `archivo:línea` leído en esta sesión. Lo que no se ha leído se dice ("no lo he leído") y se va a leer; no se razona alrededor. Si algo se puede verificar barato (grep, correr un binario, un hash), se verifica.
- **Lo que yo observo es un hecho.** Si digo que algo funciona o está roto, así es. El agente investiga por qué, o anota que no coincidimos y sigue mi instrucción. Nunca lo discute con un diff o una lista de archivos. Un comportamiento se rompe por caminos que no aparecen en el diff: lifecycle, shutdown, un bug viejo que el cambio expuso.
- **Sugerir sí, empecinarse no.** El agente puede proponer otra cosa y dar sus razones una vez. Si insisto, se hace como yo digo.
- **Una instrucción repetida no es un problema de comprensión.** Si repito algo, es porque se ignoró. No se toman atajos sobre una instrucción.

**Por qué:** en QBZ (2026-09-15) mi A/B físico mostró que el DAC no se soltaba del modo exclusivo, y el agente lo puso en duda porque "la rama no tocó qbz-audio". Estaba equivocado, y me costó horas. Antes de eso (2026-09-14) terminé mandando las revisiones a otros modelos porque un agente especulaba en lugar de leer el código.

## Reglas del proyecto

- **Arquitectura:** [ADR-001](docs/adr/0001-sin-frontend-monolitico.md). El frontend solo habla con la fachada, el core no tiene Qt, el máximo es de 1,500 LOC por archivo y 10,000 por crate de UI. Si un cambio necesita romper un candado, el agente se detiene y pregunta; nunca relaja el candado.
- **Documentos:** [ADR-002](docs/adr/0002-control-de-documentos.md). Specs, planes, research y auditorías no van en este repo: van a `chilpa-docs`. Los agentes escriben solo en `chilpa-docs/borradores/`, y lo que entra se promueve con `tools/chd promote`. Aquí solo viven los ADRs.
- **Licencia:** GPL-2.0-only. No se agregan dependencias GPL-3.0, AGPL, LGPL-3.0 ni solo Apache-2.0. Ante la duda, se pregunta antes de agregarla.
- **Bit-perfect** es un invariante, no una opción. Cualquier cambio en la ruta de salida conserva o amplía sus pruebas.
- **Compilación:** esta máquina se congela si se queda sin memoria. Una sola compilación de Rust a la vez en toda la máquina, entre todos los agentes. Nunca se lanzan `cargo build`, `check` o `test` en paralelo.
