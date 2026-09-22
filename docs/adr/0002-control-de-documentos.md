# ADR-002 — Control de documentos

- **Estado:** aceptado
- **Fecha:** 2026-09-22

## Contexto

En QBZ los documentos se volvieron un cagadero: 1,281 markdowns en `qbz-nix-docs`, 1,407 copiados en `qbz-worktrees` y 43 specs de agentes abandonadas en `.superpowers/sdd`. Hubo tres causas:

1. Las versiones vivían en el nombre del archivo (`v2`, `final`, `old`) en lugar de en git.
2. Nada distinguía un borrador de un documento vigente.
3. Los planes de los agentes entraban al repo solos y nadie los promovía ni los tiraba.

## Decisión

Los documentos internos viven en un repo privado, **`chilpa-docs`**, en el Forgejo local (`forgejo:blitzkriegfc/chilpa-docs.git`). No se publican.

Los ADRs se quedan en este repo (`docs/adr/`), porque son reglas del código y los agentes las leen junto a él.

### Reglas

1. **Un documento = un archivo = un ID, para siempre.** El nombre es `CHD-NNNN-slug.md`. Una versión nueva es un commit; nunca se crea un archivo "v2". Si un documento reemplaza a otro, lleva un ID nuevo y los dos se apuntan entre sí (`supersedes` / `superseded_by`).
2. **Cada documento lleva un encabezado** con `id`, `title`, `type`, `status`, `created` y `updated`. Opcionalmente lleva `applies_to`, `supersedes` y `superseded_by`.
    - `type`: `spec`, `research`, `plan`, `audit`, `runbook` o `release`.
    - `status`: `borrador`, `activo`, `reemplazado` o `archivado`.
3. **La estructura es por tipo, no por fecha.** Hay una carpeta por tipo y una `archivo/`. Archivar es `git mv` a `archivo/`, y el documento conserva su ID.
4. **Los documentos se amarran a lo que se libera.** Cada release de Chilpa `vX.Y.Z` tiene su documento en `release/` con los documentos activos, y el tag `chilpa-vX.Y.Z` en `chilpa-docs` congela el estado de ese momento.
5. **Lo de los agentes no entra solo.** Los agentes escriben en `borradores/`, que está en `.gitignore`. Solo entra al repo lo que se promueve con `chd promote`.

### Herramienta y candados

- **`chd`** (en `tools/chd` de `chilpa-docs`) hace todas las operaciones: `new`, `promote`, `activate`, `supersede`, `archive`, `release`, `index` y `lint`. Nadie tiene que acordarse de las reglas.
- **`INDEX.md` se genera solo**; nunca se edita a mano.
- **Un hook de pre-commit** corre `chd lint` y regenera el índice. El commit se rechaza si:
    - un nombre trae `v2`, `final`, `old`, `copy`, `backup` o `draft`;
    - hay IDs duplicados;
    - falta el encabezado o tiene valores inválidos;
    - `type` no coincide con la carpeta;
    - un documento `archivado` no está en `archivo/`;
    - un `reemplazado` no dice por cuál;
    - un `supersedes` o `superseded_by` apunta a un ID que no existe;
    - hay algo de `borradores/` en git.

## Consecuencias

- La historia de cada documento está en `git log` y no en su nombre.
- Para saber qué decía la documentación en una versión, basta con hacer checkout del tag `chilpa-vX.Y.Z`.
- Los agentes tienen un solo lugar para escribir borradores, y ese lugar no ensucia el repo.
- El hook vive en `hooks/`. Cada clon lo activa con `git config core.hooksPath hooks`, y `chd` avisa si falta.
