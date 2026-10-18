# macOS packages

This beta targets Apple Silicon. Build requirements are Rust,
Qt 6.11 with `qmake` and `macdeployqt` on PATH, and Python 3.11 or later.
Packaging requests macOS 14 as the deployment target, then raises the bundle's
minimum to the strictest requirement of its deployed libraries. The actual
minimum is recorded in `Info.plist` and the package manifest; the local Homebrew Qt build
requires macOS 26.

```sh
NORUN=1 ./scripts/qt-run.sh
python3 scripts/package-macos.py --binary target/release/chilpa-qt \
  --version 0.1.0-beta --output dist --require-native-icon
python3 scripts/check-macos-package.py dist/Chilpa.app --report dist/macos-smoke.json
```

Use Xcode 26 or later to compile `AppIcon.icon` into a native light/dark
icon catalog. With only Command Line Tools, omit `--require-native-icon`:
the package carries both ICNS variants and the running app follows system
appearance. Finder uses the light fallback in that configuration.

Packaging deploys Qt and its QML imports inside `Chilpa.app`, verifies that
Mach-O dependencies do not reference a build machine, and produces ZIP and
DMG downloads with SHA-256 checksums. These packages are signed ad hoc and are
not notarized with an Apple Developer identity.

The same GitHub Actions release workflow builds Linux and macOS assets. A manual run uploads artifacts; a version tag waits for both
platforms and creates one draft release.
