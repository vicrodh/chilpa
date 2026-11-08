# Flatpak build notes

What a Flatpak build of Chilpa has to do by hand because `scripts/qt-run.sh`
and the Linux release packaging do it outside a manifest. This is not a
manifest. It lists build steps, environment, install paths and blockers, each
with the file it comes from.

Marks:

- **[read]**: taken from the cited file and line in this repository.
- **[unverified]**: inferred, or depends on the runtime or on flatpak-builder;
  it needs a real Flatpak build to confirm.

The target is `org.kde.Platform` / `org.kde.Sdk` 6.11 with sources generated
by flatpak-cargo-generator and an offline build.

## 1. Cargo builds

Three packages are built, each with its own `-p`. Neither helper is
a dependency of `chilpa-qt`, so building `chilpa-qt` alone leaves them out.

| Package | Command used today | Source |
| --- | --- | --- |
| `chilpa-qt` | `cargo build -p chilpa-qt --release` | [read] `scripts/qt-run.sh:87` |
| `chilpa-qt` (release CI) | `cargo build --locked --release -p chilpa-qt` | [read] `.github/workflows/release-linux.yml:142` |
| `chilpa-web-sign-in` | `cargo build -p chilpa-web-sign-in --features webengine --release`, only when `pkg-config --atleast-version=6.9 Qt6WebEngineWidgets` succeeds | [read] `scripts/qt-run.sh:95-97` |
| `chilpa-plugin-runner` | not built by `qt-run.sh` nor by the release workflow; binary name `chilpa-plugin-runner` | [read] `crates/chilpa-plugin-runner/Cargo.toml:9-11` |

- Toolchain: Rust 1.96.0 in CI ([read] `release-linux.yml:113-115`),
  `rust-version = "1.96"`, edition 2021 ([read] `Cargo.toml:67-69`). There is
  no `rust-toolchain` file and no `[profile.*]` section.
- Release CI sets
  `RUSTFLAGS="--remap-path-prefix=$GITHUB_WORKSPACE=. --remap-path-prefix=$HOME/.cargo=/cargo"`
  ([read] `release-linux.yml:141`). `qt-run.sh` sets no `RUSTFLAGS`.
- `qt-run.sh` maps `JOBS` to `CARGO_BUILD_JOBS` ([read] `:73-76`); release CI
  uses `CARGO_BUILD_JOBS=2` ([read] `release-linux.yml:68`).
- Steps of `qt-run.sh` that a Flatpak build does not need: the `tools/lint/fast`
  run ([read] `:68-71`), the guard against another cargo process
  ([read] `:60-66`), `cargo metadata` piped to python3 ([read] `:78-81`), and
  the artifact directory with its git commit and dirty state
  ([read] `:105-143`). One step inside that block is needed: when
  `target/compat-runtime` exists, `scripts/package-compat.sh` copies it beside
  the binary as `compat-runtime` ([read] `:128-131`), which is where the
  compatibility worker looks for it (section 5).

## 2. Environment variables read at build time

| Variable | Read by | Effect |
| --- | --- | --- |
| `QMAKE` | [read] `crates/chilpa-qt-graphics/build.rs:30,47`; the vendored `qt-build-utils` ([read] `crates/chilpa-qt-kit/vendor/qt-build-utils/src/lib.rs:302-322`) | Path to `qmake`. Without it, `qmake6` and then `qmake` are looked up on `PATH` (`qt-build-utils` also tries `qmake-qt5` between them, [read] `:325-361`). If set to a wrong path, `qt-build-utils` fails and does not fall back. |
| `PKG_CONFIG_PATH` | [read] `crates/chilpa-web-sign-in/build.rs:13` | Where `Qt6WebEngineWidgets.pc` is found. |
| `SOURCE_DATE_EPOCH` | [read] `crates/chilpa-qt-about/build.rs:50-55` | Date of the About window's Build row; the clock when unset. |
| `CHILPA_BUILD_COMMIT` | [read] `crates/chilpa-qt-about/build.rs:51,56-62`, `build/stamp.rs:13,28` | Commit of the Build row, used when `git` is missing or its work tree is not this workspace (a source tree without `.git`). A full hex commit is cut to 9 digits; letters, digits and `.-_+` only, at most 64 characters. Git still wins when the tree is a git checkout. |
| `CXX_QT_QT_MODULES` | [read] `cxx-qt-build-0.7.3/src/lib.rs:302-303` (registry) | Not set by the repository. |

Whether flatpak-builder sets `SOURCE_DATE_EPOCH` on its own: [unverified].

## 3. Qt

- Qt is found through `qmake -query` (`QT_INSTALL_HEADERS`, `QT_VERSION`,
  `QT_HOST_LIBEXECS`, `QT_HOST_BINS`, `QT_INSTALL_LIBEXECS`, `QT_INSTALL_BINS`)
  ([read] `qt-build-utils/src/lib.rs:307-310,618-633`). moc, rcc,
  qmltyperegistrar and qmlcachegen come from those paths.
- The vendored `qt-build-utils` is upstream 0.7.3 with one change: moc gets
  `--include qqmlregistration.h` when the module list contains `Qml`
  ([read] `qt-build-utils/src/lib.rs:678-682`, `ORIGIN.json`). It is applied
  through `[patch.crates-io]` with a path ([read] `Cargo.toml:130-131`).
- Qt modules linked: Core, Gui, Network, Qml, Quick, QuickControls2
  ([read] `crates/chilpa-qt/build.rs:43-47`, `crates/chilpa-qt-artwork/build.rs:35-38`,
  and `cxx-qt-lib` with `qt_full`). QtTest only in a test
  (`crates/chilpa-qt-devices/tests/storage_events.cpp`).
- QML imports used at runtime: QtQuick, QtQuick.Controls.Basic,
  QtQuick.Effects, QtQuick.Window, QtQuick.Dialogs, QtQuick.Controls.impl,
  QtQml, QtQml.Models ([read] `crates/*/qml`). SVG images are drawn from QML,
  so the QtSvg image format plugin is needed ([unverified]; the AUR package
  depends on `qt6-svg`, [read] `packaging/aur/chilpa-bin/PKGBUILD:22`).
- `crates/chilpa-qt-graphics/build.rs`:
  - `build_scene_items()` requires Qt and fails the build without it:
    `QtBuild::new(...).expect("Qt6 is required for the scene items")`
    ([read] `:99-100`). It runs moc on six headers and includes
    `<rhi/qrhi.h>` ([read] `:106-113`; `cxx/linebed_item.cpp:25`). That header
    is in QtGui's public include folder since Qt 6.6 ([unverified]).
  - Private headers: `<headers>/{QtCore,QtGui,QtQml,QtQuick}/<version>` and
    `.../<version>/<Module>` are added only when the folder exists
    ([read] `:46-62`, `:118-134`). The only private header used is
    `private/qvulkandefaultinstance_p.h`, together with `vulkan/vulkan.h`,
    behind `__has_include` ([read] `cxx/probe.cpp:18-21`). Without them the
    build succeeds and the Vulkan device list is empty, leaving the renderer
    on Auto ([read] `build.rs:42-45`). Whether org.kde.Sdk ships QtGui's
    private headers and the Vulkan headers: [unverified].
- `chilpa-web-sign-in` with `--features webengine` probes pkg-config
  `Qt6WebEngineWidgets` at 6.9 or later and panics without it
  ([read] `crates/chilpa-web-sign-in/build.rs:16-20`). Qt WebEngine is not part
  of org.kde.Platform; it normally comes from the
  `io.qt.qtwebengine.BaseApp` base app ([unverified]). At runtime the window
  needs `QtWebEngineProcess` from `QTWEBENGINEPROCESS_PATH`, Qt's
  LibraryExecutables or Binaries path, or the executable's folder
  ([read] `cxx/window.cpp:138-152`).

## 4. Other build-time libraries (pkg-config)

| Module | Required by | When missing |
| --- | --- | --- |
| `libpipewire-0.3` >= 1.0 | [read] `crates/chilpa-pipewire/build.rs:5-9,33-36` | build fails |
| `alsa` | `alsa-sys` 0.6.1 `build.rs:7-14` (registry) | build fails |
| `dbus-1` >= 1.6 | `libdbus-sys` 0.2.7 `build.rs:15-26` (registry), through `crates/chilpa-alsa/Cargo.toml:13-14` | build fails |
| `jack` | `jack-sys` 0.5.1 `build.rs:11-14` (registry): `pkg_config::find_library("jack")` is unwrapped on Linux | build fails, although libjack is opened with dlopen at runtime |
| `libpulse`, `libpulse-simple` | `libpulse-sys` 1.23.0 `build.rs` (registry), through `crates/chilpa-pulse/Cargo.toml:15-16` | falls back to plain `-l` flags |
| `sqlite3` >= 3.34.1 | `libsqlite3-sys` 0.38.2 `build.rs:490-507` (registry); no crate enables `bundled` | falls back to `-lsqlite3` |
| OpenSSL | `openssl-sys` through native-tls ([read] `crates/chilpa-cast/Cargo.toml:15,26-27`) | [unverified]; `openssl-sys/build.rs` was not read |

`openssl-src` is in `Cargo.lock` ([read] `Cargo.lock:3367`) only for the
Android target's `vendored` feature ([read] `crates/chilpa-cast/Cargo.toml:20-21`).
It is not built on Linux ([unverified]), but flatpak-cargo-generator lists it
because it is in the lock file.

Whether org.kde.Sdk 6.11 ships `jack.pc` and the runtime ships
`libjack.so.0`: [unverified]. This is the first build blocker to check.

## 5. Install layout

Paths under `/app` follow the Flatpak convention. Sources are
repository-relative.

| Source | Install to | Notes |
| --- | --- | --- |
| `target/release/chilpa-qt` | `/app/bin/chilpa-qt` | |
| `target/release/chilpa-web-sign-in` | `/app/bin/chilpa-web-sign-in` | Found beside `chilpa-qt` with `current_exe().with_file_name(...)` and used only if it is a file ([read] `crates/chilpa-api/src/web_sign_in.rs:17,63-67`). Without it plugin sign-in links open in the browser ([read] `scripts/qt-run.sh:89-92`). |
| `target/release/chilpa-plugin-runner` | `/app/libexec/chilpa-plugin-runner` | Hard-coded path ([read] `crates/chilpa-bwrap/src/flatpak.rs:23`). Inside a Flatpak the launcher selects the portal backend ([read] `crates/chilpa-bwrap/src/process.rs:64-73`), which starts every process plugin and the compatibility worker behind this runner. Without the file the backend answers "missing program" and no process plugin starts ([read] `flatpak.rs:109-111`). |
| `target/release/compat-runtime` (from `scripts/package-compat.sh`) | `/app/bin/compat-runtime` | The compatibility worker's `adapter/worker.pl` and `libraries/lib/perl5`, found beside the executable ([read] `crates/chilpa-bwrap/src/linux.rs:29-32`). `package-compat.sh` copies `compat/perl` as `adapter/` and the `libraries/` of the runtime folder it is given ([read] `scripts/package-compat.sh:4-15`); no script in the repository produces that runtime folder. |
| Perl, built against the runtime's glibc | `/app/bin/perl` | Hard-coded path ([read] `crates/chilpa-bwrap/src/flatpak_worker.rs:21-22`); the runtime has no Perl. Without it, or without the adapter or its modules, the compatible plugins section says which is missing ([read] `flatpak_worker.rs:40-46`). |
| `packaging/linux/app.chilpa.Chilpa.desktop` | `/app/share/applications/app.chilpa.Chilpa.desktop` | Its `Exec=/usr/bin/chilpa-qt %U` ([read] `:8`) has to become `Exec=chilpa-qt %U`, as its comment says ([read] `:5-7`) and as the AppImage build rewrites it ([read] `scripts/package-linux.py:113`). `Icon=app.chilpa.Chilpa`, `StartupWMClass=chilpa-qt`. The window takes the desktop file name `app.chilpa.Chilpa` only when Qt finds that file in its applications folders ([read] `crates/chilpa-qt/src/main.rs:121-125`). |
| `packaging/linux/app.chilpa.Chilpa.svg` | `/app/share/icons/hicolor/scalable/apps/app.chilpa.Chilpa.svg` | The scalable application icon ([read] `packaging/linux/README.md`, "Desktop metadata"). The tarball and the AppImage install `assets/logo.png`, a 512x512 PNG, instead ([read] `scripts/package-linux.py:83-85,115`). |
| `packaging/linux/app.chilpa.Chilpa.metainfo.xml` | `/app/share/metainfo/app.chilpa.Chilpa.metainfo.xml` | Its `<launchable>` names `app.chilpa.Chilpa.desktop` ([read] `:40`). Checked with `appstreamcli validate --strict`, and the desktop entry with `desktop-file-validate` ([read] `packaging/linux/README.md`). |

Not installed in a Flatpak:

- `packaging/linux/chilpa-restart`, the AppImage updater and `release.json`:
  inside a sandbox the updates view takes the store's place and reads none of
  them; it checks and downloads nothing ([read]
  `crates/chilpa-api/src/appimage_updates.rs:24-40`). Outside one, the AppImage
  updater needs `$APPIMAGE` ([read] `:43`, `crates/chilpa-dirs/src/lib.rs:190-193`)
  and `release.json` turns on the tarball's self-update
  ([read] `crates/chilpa-api/src/bundle_updates.rs:57-66`).
- `packaging/linux/chilpa-mini-player-keep-above.kwinrule`: imported by hand,
  never installed by a package ([read] its lines 16-18).

Compiled into `chilpa-qt`, nothing to install:

- QML and its resources, as cxx-qt QML modules loaded from
  `qrc:/qt/qml/app/chilpa/...` ([read] `crates/chilpa-qt/build.rs:26-30,48-54`,
  `crates/chilpa-qt/src/main.rs:165`).
- Translations: the eight `.po` catalogs through `include_str!`
  ([read] `crates/chilpa-i18n/src/lib.rs:26-35`).
- Fonts `Inter.ttf` and `LINESeedJP-Regular.ttf` in the kit's resources
  ([read] `crates/chilpa-qt-kit/build.rs:32`, `crates/chilpa-qt/src/main.rs:69,72`).
- Icons and tray pixmaps ([read] `crates/chilpa-mpris/src/tray.rs:26-41`).
- The plugin index, through
  `include_bytes!("../../../packaging/plugins/index.json")`
  ([read] `crates/chilpa-api/src/plugin_repositories.rs:18`). The build
  therefore needs the whole repository, not only `crates/`.

## 6. License files

The AppImage installs these under `usr/share/licenses/chilpa/`
([read] `scripts/package-linux.py:152-163`); the tarball puts the first group
under `licenses/` ([read] `:84-91`). For the Flatpak the folder is
`/app/share/licenses/app.chilpa.Chilpa/`.

| Source | Installed name |
| --- | --- |
| `LICENSE` | `LICENSE` |
| `licenses/` (10 files: `Apache-2.0-dst-decoder.txt`, `dsd2pcm-BSD-2-Clause.txt`, `dst-decoder-NOTICE.md`, `dst-decoder-upstream-NOTICE.txt`, `genre-lists.txt`, `immersive-renderers-MIT.txt`, `inter-OFL.txt`, `lucide-icons.txt`, `source-icons.txt`, `tabler-icons.txt`) | same names |
| `vendor/chilpa-plugin-api/LICENSE` | `plugin-protocol-MIT.txt` |
| `packaging/linux/Inter-OFL.txt` | `Inter-OFL.txt` (byte-identical to `licenses/inter-OFL.txt`) |

AppImage only, because it bundles Qt and the updater: `Qt-LGPL-3.0.txt`,
`Qt-GPL-3.0.txt`, a generated `Qt-source.txt`, Qt's own `Licenses/` folder and
`AppImageUpdate.txt` ([read] `scripts/package-linux.py:156-163`). In a Flatpak
Qt comes from the runtime.

Files the packaging scripts do not install, found in the tree:

- `crates/chilpa-qt-sources/third-party/LICENSE-circle-flags.md`
- `crates/chilpa-qt-kit/vendor/qt-build-utils/LICENSE-APACHE` and `NOTICE`

There is no generated notices file for the Rust crates in the lock file: no
cargo-about configuration and no checked-in notices file. `tools/lint/licencias`
only reports from `cargo metadata`, and `deny.toml` configures cargo-deny.
No license text for the embedded LINE Seed JP font was found in the tree.

## 7. Offline build with vendored sources

- `Cargo.lock`: 512 crates.io registry entries, no `git+` source and no other
  registry. `qt-build-utils` and `chilpa-plugin-api` are path entries
  ([read] `Cargo.lock:737-740,3568-3574`).
- Workspace path crates outside `crates/`: `vendor/chilpa-plugin-api`
  (depends only on serde) and `crates/chilpa-qt-kit/vendor/qt-build-utils`,
  both excluded from the workspace and present in the tree
  ([read] `Cargo.toml:3,128,130-131`).
- No crate declares a `git =` dependency or a `path =` outside the repository.
- `.cargo/config.toml` only sets `[net] git-fetch-with-cli = true`
  ([read] `.cargo/config.toml:2-3`); with no git dependencies it has no
  effect. A vendored build adds its own source replacement next to it.
- No build script reaches the network. The only programs build scripts run
  are `git` ([read] `crates/chilpa-qt-about/build.rs:36-41`, optional), `qmake`
  ([read] `crates/chilpa-qt-graphics/build.rs:32`) and Qt's host tools through
  `qt-build-utils`.
- A `[patch.crates-io]` entry with a path works together with a source
  replacement to a vendored directory, so `cargo build --offline --locked`
  works in principle ([unverified]).
- `cargo-sources.json` has to be generated from the `Cargo.lock` of the
  commit being built; `--locked` refuses a lock file that is out of date.

## 8. What the code expects from the sandbox at runtime

Every check for a Flatpak goes through `chilpa_dirs::sandbox()`, which
reads `/.flatpak-info` once ([read] `crates/chilpa-dirs/src/lib.rs:148-151`).
`chilpa-pipewire` keeps its own copy of that check behind the playback shield
([read] `crates/chilpa-pipewire/src/setup.rs:160-177`).

- Credentials: inside a Flatpak the box asks the desktop portal's
  `org.freedesktop.portal.Secret` for the app's value and never the Secret
  Service; without an answer within 30 s it falls back to a key derived from
  `/etc/machine-id` and an install id
  ([read] `crates/chilpa-secrets/src/backend.rs:96-106`, `src/portal.rs`).
  Which desktops serve that portal interface (gnome-keyring, KWallet) on the
  target systems: [unverified].
- Process plugins and the compatibility worker:
  `org.freedesktop.portal.Flatpak` `Spawn`, version 3 or later
  ([read] `crates/chilpa-bwrap/src/portal.rs:27-32`), with the runner and Perl
  of section 5.
- Other portals, all on the session bus: `Inhibit` keeps the machine awake
  while playing, where logind is used outside a sandbox
  ([read] `crates/chilpa-mpris/src/inhibit.rs:28,80`); `Settings` gives the
  colour scheme and accent ([read] `crates/chilpa-mpris/src/portal.rs:21`,
  `crates/chilpa-api/src/sandbox.rs:83-89`); `Documents` maps a folder chosen
  through the file chooser to its host path ([read] `portal.rs:23`);
  `Realtime` raises the thread that feeds an ALSA device when RealtimeKit on
  the system bus does not answer ([read] `crates/chilpa-alsa/src/realtime.rs:16-19,40`).
  No portal names the desktop's wallpaper, so the wallpaper background has
  none in a sandbox ([read] `crates/chilpa-api/src/wallpaper.rs:24`).
- Devices and sockets the audio stacks name when they are missing, each as a
  `flatpak override` the user can run: `--device=all` for the cards in
  `/dev/snd`, without which the direct stacks offer no card
  ([read] `crates/chilpa-alsa/src/backends.rs:70-75`), and
  `--filesystem=xdg-run/pipewire-0` for PipeWire's own socket, which its ALSA
  PCM, the native DSD stream and its JACK library connect to
  ([read] `crates/chilpa-pipewire/src/lib.rs:252-258`,
  `crates/chilpa-jack/src/lib.rs:285`). `pactl` talks to the PulseAudio
  socket instead, so the PipeWire stack also checks for PipeWire's own socket
  after `pactl` has answered ([read] `crates/chilpa-pipewire/src/lib.rs:398-400,440`).
- Device sync finds memory cards under `/run/media` or `/media`
  ([read] `crates/chilpa-dirs/src/lib.rs:182-187`); when neither is visible
  it says so and offers a folder chosen on the card
  ([read] `crates/chilpa-api/src/sandbox.rs:182-197`).
- Programs run by name, all optional (a missing one is tolerated): `pactl`,
  `pw-metadata`, `pw-dump`, `aplay`, `systemctl --user`, `pgrep`,
  `gsettings`, `xfconf-query`, `adb` ([read] `crates/chilpa-pipewire/src/lib.rs:84,93-105`,
  `src/setup.rs:41-52`, `crates/chilpa-dirs/src/lib.rs:99`,
  `crates/chilpa-devices/src/transport/adb.rs:13`). The HiFi wizard reads
  WirePlumber's clients out of `pw-dump` and ALSA's own list of PCMs before it
  asks `systemctl`, `pgrep` or `aplay` ([read] `src/setup.rs:32-79`). In a
  sandbox the desktop's colours come from the `Settings` portal and not from
  `gsettings` ([read] `crates/chilpa-api/src/sandbox.rs:83-89`). Which of
  these programs exist in the runtime: [unverified].
- Linked system libraries: Qt, libpipewire-0.3, libasound, libpulse and
  libpulse-simple, libdbus-1, libsqlite3, OpenSSL. libjack is opened with
  dlopen as `libjack.so.0` ([read] `jack-sys` 0.5.1 `src/consts.rs:17`,
  registry). Decoding, CD reading and TLS for streams are pure Rust, and so
  is D-Bus for MPRIS and the Inhibit, Settings, Documents, Secret and Flatpak
  portals (zbus). RealtimeKit and the Realtime portal go through libdbus-1
  ([read] `crates/chilpa-alsa/src/realtime.rs:23-24`,
  `crates/chilpa-alsa/Cargo.toml:13`).

## 9. Blockers, in the order a build meets them

1. `jack-sys` needs `jack.pc` at build time on Linux (section 4).
   [unverified] whether the SDK has it.
2. `libpipewire-0.3` >= 1.0, `alsa` and `dbus-1` >= 1.6 must be found by
   pkg-config (section 4).
3. `qmake` (or `qmake6`) must answer for the SDK's Qt, or `QMAKE` must point
   at it; a wrong `QMAKE` is fatal (section 2).
4. `chilpa-web-sign-in --features webengine` needs `Qt6WebEngineWidgets` 6.9
   or later on `PKG_CONFIG_PATH`; without Qt WebEngine, build the other two
   packages only.
5. Without `.git` in the build directory, set `CHILPA_BUILD_COMMIT`, or the
   About window names no commit; set `SOURCE_DATE_EPOCH` for a reproducible
   build date.
6. `chilpa-plugin-runner` is not built by any existing script; it needs its
   own `cargo build -p chilpa-plugin-runner --release` and the
   `/app/libexec` path. Inside a Flatpak every process plugin starts through
   the portal behind it, so without it none starts (section 5).
7. The compatibility worker needs Perl at `/app/bin/perl`, built for the
   runtime, and `compat-runtime` with its Perl modules beside `chilpa-qt`.
   No script in the repository builds either for the runtime (section 5).

## 10. Known limits of the sandbox boundary

What the code cannot close from inside the Flatpak, each with what bounds it
today. Read from the code at the cited lines; the portal's and bubblewrap's
behaviour from their sources (flatpak `portal/flatpak-portal.c`, bubblewrap
`bubblewrap.c`, main branches, read 2026-11-07; RealtimeKit's
`rtkit-daemon.c`, read 2026-11-08).

- **The portal forgets a process at its exit.** When the process it started
  exits, the portal emits `SpawnExited`, drops the pid and answers a later
  `SpawnSignal` with "No such pid" (`child_watch_died`,
  `handle_spawn_signal`). Its process is bubblewrap's outer one, which exits
  when the sandbox's first process exits while the sandbox's pid 1 keeps
  running with whatever that process left (`monitor_child`, `do_init`). So
  `chilpa-plugin-runner` does not exec the plugin: it runs it as a child,
  adopts its orphans as a child subreaper and ends them before exiting
  ([read] `crates/chilpa-plugin-runner/src/lib.rs`, `supervise` and
  `end_all`). Whether Flatpak's seccomp filter allows
  `PR_SET_CHILD_SUBREAPER`: [unverified]. If it refuses, the runner exits
  with 126 and no process plugin starts. Before the plugin starts, the
  runner closes its own descriptors, wipes its environment and becomes
  undumpable, so the plugin, which runs as the same user, reads neither
  through `/proc/$PPID` ([read] `lib.rs`, `conceal`).
- **A plugin that kills its runner.** The plugin runs as the runner's own
  user and can `kill(getppid(), SIGKILL)`; nothing in the sandbox can
  forbid it. The plugin itself dies with the runner (`PR_SET_PDEATHSIG`,
  [read] `lib.rs`, `die_with`), but what the plugin started on its own has
  no such tie: it is left under the sandbox's pid 1, and the portal, which
  saw its process exit, no longer answers for it. It ends when Chilpa
  leaves the bus only if it does not ignore SIGINT. Closing this needs the
  sandbox's pid 1 to end what it holds when the runner goes, which
  bubblewrap does only with `--die-with-parent`, an option the portal does
  not pass.
- **Chilpa leaving the bus sends SIGINT, not SIGKILL.** `WATCH_BUS` ends
  each sandbox with `killpg(pid, SIGINT)` (`name_owner_changed`). The runner
  hands SIGINT, SIGTERM and SIGHUP on to the plugin, gives it 2 s to exit
  and then kills everything below it; a plugin that ignores SIGINT is still
  ended that way ([read] `lib.rs`, `supervise`).
- **End of output while the plugin lives: a test for a real Flatpak.** The
  runner lets go of its copies of the plugin's standard input and output
  ([read] `lib.rs`, `release_stdio`), so a plugin that closes its standard
  output gives Chilpa end of file outside a sandbox. Under the portal,
  bubblewrap's monitor and the sandbox's pid 1 may hold copies too:
  [unverified]. To check inside the Chilpa Flatpak: a process plugin that
  answers `initialize`, then closes its standard output and sleeps; the
  plugin's channel should report it closed within a second, not when the
  sleep ends.
- **A spawn the portal never answers.** A `Spawn` answered after its caller
  gave up (10 s, [read] `crates/chilpa-bwrap/src/portal.rs:77-79`) is ended
  when the answer comes, also when the ledger no longer keeps that call or
  has no room left to track the process; one never answered runs until
  Chilpa leaves the bus. A signal the portal refuses is sent again while the process is
  waited for, and a process whose handle goes while it may still run stays
  in the ledger and is signalled from a thread of its own ([read]
  `portal.rs`, `wait`, `Drop for PortalProcess`, `end_abandoned`).
- **The compatibility worker's /tmp has no total bound.** Bubblewrap mounts
  a 16 MiB tmpfs there ([read] `crates/chilpa-bwrap/src/linux_bwrap.rs:68`);
  the portal has no option for one. What bounds the worker is what the
  runner sets ([read] `crates/chilpa-bwrap/src/flatpak_worker.rs:91-93`):
  no forks (`--nproc=0`), 1 MiB per file (`--fsize`), 64 descriptors and
  120 s of CPU in all. They bound each file and the time spent writing, not
  the sum of the files: a worker that writes and closes files in a loop can
  fill the sandbox's /tmp until its CPU runs out. Lowering them does not
  change that order of magnitude, since writing a tmpfs takes little CPU
  per byte, and it would cut the worker's legitimate work first; they stay
  as bubblewrap's. Where that /tmp lives for a sandboxed spawn (a tmpfs of
  its own or a folder under the user's runtime directory), and so what
  fills up: [unverified].
- **Locks held across portal calls.** `Natives::lifecycle` is held while a
  plugin is stopped and started ([read] `crates/chilpa-api/src/natives.rs:77-78`,
  `:265-266`, `:276-277`), and enabling, disabling, updating and granting
  network hold `Mutex<Plugins>` around `reload_one`, which stops and starts
  ([read] `crates/chilpa-api/src/plugin_center.rs:393-398`,
  `crates/chilpa-api/src/plugin_updates.rs:35`,
  `crates/chilpa-api/src/plugins.rs:352-354`). Each wait has a bound: 3 s
  for a `SpawnSignal` answer and 10 s for the exit, then 2 s for a worker's
  pipe threads ([read] `crates/chilpa-plugin/src/pipe_transport.rs:16`);
  10 s for a `Spawn` answer and 15 s for the plugin's greeting
  ([read] `natives.rs:24`). A slow portal therefore holds plugin management
  for up to about 40 s per plugin, and a network change replaces the
  plugins one after another under the lifecycle lock. Moving the portal
  calls out of these locks would let stops, starts, updates and restarts
  of the same plugin interleave, which the restart's generation check
  relies on not happening; it is not a small change and is not made here.
- **Real time through the portal lowers the hard time limit for good.**
  RealtimeKit reads the hard `RLIMIT_RTTIME` of the process it is asked for
  and refuses above its `RTTimeUSecMax` (200 ms by default), before it
  grants and again after (`verify_process_rttime`); the soft limit alone
  does not do. So a hard limit above that is lowered to it before the
  portal is asked, and one already lower is left as it is ([read]
  `crates/chilpa-alsa/src/realtime.rs`, `granted_by_portal`, `rt_time_for`).
  If the portal then refuses, the hard limit cannot go back up without
  CAP_SYS_RESOURCE; the soft one comes back within it. What stays lowered
  bounds only real-time threads, and inside a Flatpak every real-time grant
  comes from RealtimeKit, which requires the same limit; ordinary threads
  are not bounded by it.
