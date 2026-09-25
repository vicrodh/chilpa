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
- `tools/lint/` — the checks that enforce them.

Agent rules, design decisions, plans and internal documents are not part of
this repository. Public docs for contributors and plugin authors will live
here once the project opens up.

Before working on this repo:

```
git config core.hooksPath hooks
```

`tools/lint/fast` runs the cheap checks and the pre-commit hook runs it on
every commit; `tools/lint/all` adds the idle-memory budget, which costs a few
seconds and needs a built binary.

## License

MIT. See [LICENSE](LICENSE).

Some optional features link libraries under copyleft licenses. When a build
includes those, the resulting binary ships under their terms; the source in
this repository stays MIT either way.

## Legal

- **Your own music.** Chilpa plays the files, discs and disc images you give
  it. It ships no music, no disc images and no keys.
- **SACD images.** Chilpa reads the stereo area of Super Audio CD images that
  are not encrypted. It has no way to decrypt a disc or an image and does not
  try, and it contains nothing that gets around copy protection. An image it
  plays is one somebody already made from a disc they have.
- **Copying discs.** Ripping a CD, or making an image of a disc, may or may
  not be allowed where you live, even for your own discs. Checking that is up
  to you.
- **Trademarks.** Super Audio CD, SACD, Direct Stream Digital and DSD are
  trademarks of their respective owners. Chilpa is not affiliated with,
  endorsed by or certified by them, and uses the names only to say which
  formats it reads.
