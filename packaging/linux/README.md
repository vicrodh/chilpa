# Linux releases

I build Linux x86_64 and aarch64 releases as a binary tarball, DEB, RPM and AppImage. The workflow builds
on Ubuntu 24.04 with Qt 6.11.2 and Rust 1.96.0. The tarball needs system Qt and
audio libraries; DEB, RPM and AppImage include the same deployed Qt Quick and audio runtime.
I install that runtime under `/opt/chilpa` in DEB/RPM, with a symlink at `/usr/bin/chilpa-qt`.
Packaging runs on Ubuntu 24.04 and checks every bundled ELF file against glibc 2.39.
Checking the application alone misses newer requirements in Qt and its dependencies.
The AppImage uses the system graphics stack, including OpenGL, EGL and font libraries.
The Wayland platform, shell and graphics integration plugins are bundled together;
the platform plugin alone cannot open a native Wayland window.
Install `bubblewrap` for process plugins and run them as a normal user with
user namespaces available. Packaging tools extract without requiring FUSE.

## Release workflow

1. Set `[workspace.package].version` in `Cargo.toml` to `X.Y.Z` or a prerelease
   such as `0.1.0-rc.1` and update `Cargo.lock` before tagging the same commit
   with a `v` prefix. Releases come only from `main`: the tagged commit must
   already be on `main`, or the workflow stops before building.
2. The tag starts `.github/workflows/release-linux.yml`. It checks the version,
   runs repository gates and API tests, builds one binary per architecture, packages all four formats,
   checks AppImage startup and verifies all checksums.
3. The workflow creates a draft GitHub release and marks prerelease tags accordingly.
   Review and publish the draft.
   A manual dispatch produces downloadable workflow artifacts without creating
   a release. `source_ref` selects the commit or tag to build.

The assets are:

- `chilpa-X.Y.Z-linux-{x86_64,aarch64}.tar.gz`
- `chilpa_X.Y.Z_{amd64,arm64}.deb`
- `chilpa-X.Y.Z-1.{x86_64,aarch64}.rpm`
- `Chilpa-X.Y.Z-{x86_64,aarch64}.AppImage` and matching `.zsync`
- `MANIFEST-linux-{x86_64,aarch64}.json`
- `install.sh`, `RELEASE-NOTES.md` and combined `SHA256SUMS`

I keep the bare tarball layout at `chilpa-X.Y.Z/{chilpa-qt,LICENSE,licenses,icons,...}`
for binary packaging in AUR and Gentoo. Those packages provide their own system Qt.
The install/update command in the README selects the AppImage on Linux and the
ZIP bundle on macOS. Running it again updates its own installation after verifying
the download; it preserves the user's library, settings and plugins.

Publish the AppImage and its matching `.zsync` together. Stable installations
only accept newer stable releases. Prerelease installations also accept newer published
prereleases and can advance to a stable release. Candidate selection uses semantic version order and rejects drafts and
mismatched prerelease metadata.

## AppImage updates

The AppImage includes the MIT AppImageUpdate command line tool. Chilpa checks in the background
after startup, at most once per day; About includes the automatic-check switch
and a manual check. A notice opens the update controls when a newer image exists.
Downloads start when the listener presses Update, with progress and cancellation. Chilpa remains
usable during the download.

The host selects a newer version from the installation's channel and pins its
tag for the download. It stages the download beside the original image, verifies
its header, size and SHA-256 against the selected GitHub release, then replaces
the file atomically. One `<original>.previous` image is retained. Restart waits for the current process to close before
launching the updated image. The library, configuration and plugins are separate
from these files. These controls appear only when the bundled updater runs from
an AppImage; the tarball stays a manual installation.

The image needs a writable parent folder. Move it to a user-owned folder before
updating an image installed in a system-owned directory. The updater uses the
HTTPS GitHub channel embedded during packaging, without account credentials.
A prerelease image's embedded fallback is pinned to its own tag; the host explicitly
selects newer eligible tags before invoking the updater. A standalone updater
therefore cannot silently cross from the prerelease fallback into the stable channel.

## Local packaging

Build the release binary, then obtain the tools with an authenticated gh:

```sh
GH_CONFIG_DIR="$HOME/.config/gh-personal" python3 scripts/fetch-release-tools.py /tmp/chilpa-release-tools
python3 scripts/package-linux.py \
  --binary target/release/chilpa-qt --version X.Y.Z \
  --repository OWNER/REPOSITORY --tools /tmp/chilpa-release-tools --output dist
```

`tools.lock.json` pins every packaging executable by release tag and SHA-256.
I use appimagetool 1.9.1 and the dated 20251108 runtime so upstream continuous
builds cannot change those downloads between releases. Update the lock when
adopting a different upstream payload. The plugin protocol is an unmodified, vendored public SDK snapshot;
`vendor/chilpa-plugin-api/ORIGIN.json` records its tag and revision. No private
repository is needed to compile.

The official plugin index is bundled as the initial offline catalog. Its public
URL remains configured by default; Refresh in the plugin manager retrieves the
current index. Installed plugins still need their normal permission approval.
