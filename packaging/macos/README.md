# macOS packages

I build this beta for Apple Silicon on macOS 14 or later. I require Rust,
Qt 6.11 with `qmake` and `macdeployqt` on PATH, and Python 3.11 or later.

```sh
NORUN=1 ./scripts/qt-run.sh
python3 scripts/package-macos.py --binary target/release/chilpa-qt \
  --version 0.1.0-beta --output dist --require-native-icon
python3 scripts/check-macos-package.py dist/Chilpa.app --report dist/macos-smoke.json
```

I use Xcode 26 or later to compile `AppIcon.icon` into a native light/dark
icon catalog. With only Command Line Tools, I omit `--require-native-icon`:
the package carries both ICNS variants and the running app follows system
appearance. Finder uses the light fallback in that configuration.

I deploy Qt and its QML imports inside `Chilpa.app`, verify that its Mach-O
dependencies do not reference a build machine, and produce ZIP and DMG
downloads with SHA-256 checksums. I sign these packages ad hoc; they are
not notarized with an Apple Developer identity.

I build the Linux and macOS assets in the same GitHub Actions release
workflow. A manual run uploads artifacts; a version tag waits for both
platforms and creates one draft release.
