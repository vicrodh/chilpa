# Linux releases

I package Linux x86_64 as a binary tarball and an AppImage. The workflow builds
on Ubuntu 24.04 with Qt 6.11.2 and Rust 1.90.0. The tarball needs system Qt and
audio libraries; the AppImage includes its Qt Quick and audio runtime.
I package on Ubuntu 24.04 and check every bundled ELF file against glibc 2.39.
Checking the application alone misses newer requirements in Qt and its dependencies.
I use the system graphics stack, including OpenGL, EGL and font libraries.
I bundle the Wayland platform, shell and graphics integration plugins together;
the platform plugin alone cannot open a native Wayland window.
I install `bubblewrap` for process plugins and run them as a normal user with
user namespaces available. I extract packaging tools without requiring FUSE.

## Release workflow

1. I set `[workspace.package].version` in `Cargo.toml` to `X.Y.Z` or a prerelease
   such as `0.1.0-beta` and update `Cargo.lock` before tagging the same commit
   with a `v` prefix.
2. The tag starts `.github/workflows/release-linux.yml`. It checks the version,
   runs repository gates and API tests, builds the binary, packages both formats,
   checks AppImage startup and verifies all checksums.
3. The workflow creates a draft GitHub release and marks beta tags as prereleases.
   I review and publish the draft.
   A manual dispatch produces downloadable workflow artifacts without creating
   a release. `source_ref` selects the commit or tag to build.

The assets are:

- `chilpa-X.Y.Z-linux-x86_64.tar.gz`
- `Chilpa-X.Y.Z-x86_64.AppImage`
- `Chilpa-X.Y.Z-x86_64.AppImage.zsync`
- `MANIFEST.json`, `RELEASE-NOTES.md` and `SHA256SUMS`

I publish the AppImage and its matching `.zsync` together. Stable installations
only accept newer stable releases. Beta installations also accept newer published
prereleases and can advance to a stable release. I order candidates by semantic
version and reject drafts and mismatched prerelease metadata.

## AppImage updates

I bundle the MIT AppImageUpdate command line tool. Chilpa checks in the background
after startup, at most once per day; About includes the automatic-check switch
and a manual check. A notice opens the update controls when a newer image exists.
Downloads start when I press Update, with progress and cancellation. Chilpa remains
usable during the download.

I select a newer version from the installation's channel and pin its tag for the download. I stage the
download beside the original image, verify its header, size and SHA-256 against
the selected GitHub release, and replace the file atomically. I retain one
`<original>.previous` image. Restart waits for the current process to close before
launching the updated image. The library, configuration and plugins are separate
from these files. These controls appear only when the bundled updater runs from
an AppImage; the tarball stays a manual installation.

The image needs a writable parent folder. I move it to a user-owned folder before
updating an image installed in a system-owned directory. The updater uses the
HTTPS GitHub channel embedded during packaging; I do not embed account credentials.
I pin the beta image's embedded fallback to its own tag; the host explicitly
selects newer eligible tags before invoking the updater. A standalone updater
therefore cannot silently cross from the beta fallback into the stable channel.

## Local packaging

I first build the release binary, then obtain the tools with an authenticated gh:

```sh
GH_CONFIG_DIR="$HOME/.config/gh-personal" python3 scripts/fetch-release-tools.py /tmp/chilpa-release-tools
python3 scripts/package-linux.py \
  --binary target/release/chilpa-qt --version X.Y.Z \
  --repository OWNER/REPOSITORY --tools /tmp/chilpa-release-tools --output dist
```

`tools.lock.json` pins every packaging executable by SHA-256. I update the lock
when adopting a different upstream payload, including changes to a continuous
release. The plugin protocol is an unmodified, vendored public SDK snapshot;
`vendor/chilpa-plugin-api/ORIGIN.json` records its tag and revision. No private
repository is needed to compile.

The official plugin index is bundled as the initial offline catalog. Its public
URL remains configured by default; Refresh in the plugin manager retrieves the
current index. Installed plugins still need their normal permission approval.
