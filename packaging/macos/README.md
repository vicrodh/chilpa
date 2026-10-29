# macOS packages

I build releases natively for Apple Silicon and Intel. Build requirements are Rust,
Qt 6.11 with `qmake` and `macdeployqt` on PATH, and Python 3.11 or later.
Packaging requests macOS 14 as the deployment target, then raises the bundle's
minimum to the strictest requirement of its deployed libraries. The actual
minimum is recorded in `Info.plist` and the package manifest; the local Homebrew Qt build
requires macOS 26.

```sh
NORUN=1 ./scripts/qt-run.sh
python3 scripts/package-macos.py --binary target/release/chilpa-qt \
  --version 0.1.0 --output dist --require-native-icon
python3 scripts/check-macos-package.py dist/Chilpa.app --report dist/macos-smoke.json
```

Use Xcode 26 or later to compile `AppIcon.icon` into a native light/dark
icon catalog. With only Command Line Tools, omit `--require-native-icon`:
the package carries both ICNS variants and the running app follows system
appearance. Finder uses the light fallback in that configuration.

The ICNS variants are made from `app-icon-macos-light.png` and `app-icon-macos-dark.png` in `crates/chilpa-qt-kit/assets`: the
artwork in a squircle body of 896 of 1024 points with a margin and a soft
shadow, which is the size of QBZ's icon. Art that fills its canvas draws
larger than every other icon in the Dock. The running app sets the same two images as its Dock icon when the bundle has no compiled icon catalog, so the Dock never shows the full-bleed art. `tools/icons/macos-icon` rebuilds
them from `crates/chilpa-qt-kit/assets/app-icon-*.png` and needs Pillow.

Packaging deploys Qt and its QML imports inside `Chilpa.app`, verifies that
Mach-O dependencies do not reference a build machine, and produces ZIP and
DMG downloads with SHA-256 checksums. These packages are signed ad hoc and are
not notarized with an Apple Developer identity.

The same GitHub Actions release workflow builds Linux and macOS assets. A manual run uploads artifacts; a version tag waits for all four
platform/architecture jobs and creates one draft release.

I publish `Chilpa-X.Y.Z-macos-{x86_64,aarch64}.zip` and `.dmg`, a manifest
for each architecture, and one combined `SHA256SUMS`. The install/update script
selects a native ZIP bundle, including from a Rosetta terminal, verifies the
release digest and ad hoc code signature, then installs into `~/Applications`.
Running the command again updates the installation it owns.
