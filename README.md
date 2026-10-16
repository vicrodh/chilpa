<div align="center">

<img width="256" height="256" alt="Chilpa" src="https://github.com/user-attachments/assets/82cf8d2b-f9a3-43bb-9e2d-12314a54ef3f" />

<br>

<img width="150" height="28" alt="Chilpa" src="https://github.com/user-attachments/assets/66a2ada1-b2dc-4383-8b11-82c95a6228da" />

</div>

# Chilpa

I build Chilpa as a native music player with local libraries, self-hosted
servers and optional plugins. I keep the audio engine independent of Qt and
put source-specific integrations in separate plugins.

*Chilpatl* is Nahuatl for hornet.

## 0.1.0-beta

I prepare the first public beta for Linux x86_64 as an AppImage and a binary
tarball. I mark it as a prerelease; I do not present it as a stable version.
I keep macOS, Windows and Android development in the workspace, but do not
provide validated binaries for those platforms in this beta.

I publish downloads in [Releases](https://github.com/vicrodh/chilpa/releases)
and progress updates in [Discussions](https://github.com/vicrodh/chilpa/discussions).

I include:

- Local music libraries, album editions, CDs and supported disc images.
- Multiple queues, playlists, search and source-aware library browsing.
- Audio output that reports its actual format and processing state.
- Synchronized lyrics, translations and immersive views that react to audio.
- A command line, configurable keyboard shortcuts and keyboard navigation.
- Optional DSP presets and graphic EQ, radio, podcasts, playlist import and
  local mood analysis through the official plugin catalog.

I keep native DSD and DoP outside DSP processing. Enabling PCM DSP changes the
signal; I show processed output rather than claiming strict bit-perfect output.

## Install

For the AppImage, I download `Chilpa-0.1.0-beta-x86_64.AppImage`, make it
executable and run it. I can use `APPIMAGE_EXTRACT_AND_RUN=1` when FUSE is
unavailable. I keep the image in a writable folder to use in-app updates.

The tarball contains `chilpa-qt` and notices. I install the system Qt and audio
dependencies before running it. I keep the library, settings and plugins in
their normal user directories; an application update does not replace them.
I include SHA-256 checksums with the downloads.

## Official plugins

I configure the [official catalog](https://github.com/vicrodh/chilpa-plugins)
by default and bundle an initial copy for offline browsing. I use Settings >
Plugins to refresh it and install individual packages. Each plugin still asks
for its declared permissions before it is enabled.

I distribute Radio, Podcasts and DSP as WASM packages. Playlist Importer and
Mood use Linux x86_64 process packages in this beta. Mood downloads optional
models only after it is enabled; I describe their separate license in its
package. The core and installer do not include model weights.
I install `bubblewrap` and enable user namespaces for process plugins on Linux.
I run Chilpa as a normal user; its process sandbox refuses root.

I document the API and authoring contracts in
[chilpa-plugins-sdk](https://github.com/vicrodh/chilpa-plugins-sdk).

## Build

I use Rust 1.90 or newer, Qt 6.11.2 with Qt Quick, a C/C++ toolchain, CMake,
pkg-config, libclang and NASM. On Linux I also need the development packages
for ALSA, JACK, PulseAudio, PipeWire, D-Bus and OpenSSL. The Linux release
workflow lists the exact packages used for the published build.

```sh
cargo build --locked --release -p chilpa-qt
./target/release/chilpa-qt
```

I vendor the public plugin protocol snapshot, so building the core needs no
private repository. I record its upstream revision in
`vendor/chilpa-plugin-api/ORIGIN.json`.

I describe checks and contributions in [CONTRIBUTING.md](CONTRIBUTING.md) and
Linux packaging in [packaging/linux/README.md](packaging/linux/README.md).

## License

I license this repository under MIT; see [LICENSE](LICENSE). I preserve
third-party notices in `licenses/` and the licenses of bundled Qt and runtime
components in the distributed packages. Plugins and optional model downloads
retain their own notices.
