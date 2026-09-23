<img src="assets/logo.png" alt="" width="140" align="right">

# Chilpa

*Chilpatl* is Nahuatl for hornet. Chilpa is a music player for the desktop.

It is platform-agnostic by design: local files, self-hosted servers and
third-party plugins are all just sources, and the core never knows which
services exist. It reuses the audio engine of QBZ, now discontinued, rebuilt
around what a deep audit of that engine found.

What it keeps from QBZ:

- Bit-perfect output as a tested invariant, not a checkbox.
- Wide format support, DSD, SACD, CD playback and secure CD ripping.
- Self-hosted servers, several accounts of each.
- Native desktop app: no Electron, no webview, no bundled browser.

What QBZ never got to:

- A plugin system from day one, sandboxed, where a broken plugin cannot take
  the player down.
- Listening stats that feed smart lists by mood, rhythm and occasion.
- Multiple layouts, and views that merge one album across every source.
- Radio, podcasts, DAP sync, MilkDrop and technical visualizers.
- A low memory and CPU footprint, measured in CI.

## Repository

- `crates/` — the workspace. `chilpa-api` is the only surface a frontend may
  use.
- `docs/adr/` — the decisions that constrain the code.
- `tools/lint/` — the checks that enforce them.

Agent rules, plans and internal documents are not part of this repository.

Before working on this repo:

```
git config core.hooksPath hooks
```

`tools/lint/all` runs every check; the pre-commit hook runs the same thing.

## License

GPL-2.0-only, with a plugin exception: a plugin that talks to Chilpa only
through the public plugin API may carry any license. See [LICENSE](LICENSE)
and [LICENSE-EXCEPTION-PLUGINS](LICENSE-EXCEPTION-PLUGINS).
