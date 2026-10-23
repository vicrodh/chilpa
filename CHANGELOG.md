# 0.1.0

Initial public release for Linux and macOS, each on x86_64 and ARM64 (Apple Silicon).

- Local libraries and connected sources in one native Qt interface.
- Destination skeletons appear before content loads to keep navigation responsive.
- Multiple queues, playlists, album editions, search and a keyboard command line.
- Synchronized lyrics and translations, with immersive audio visualizations.
- Official plugin catalog configured on first start, including an offline snapshot.
- Radio, Podcasts, Playlist Importer, DSP and Mood as separate official plugins.
- Linux AppImage, tarball, DEB and RPM packages, plus macOS ZIP and DMG packages, with checksums, provenance and third-party notices.
- Stable AppImage updates, with verification before replacing the installed image.

Third-party playback plugins and model weights are not included. Mood downloads
optional models only after it is enabled; their separate license is documented
in the plugin package. Native plugin packages are selected by platform.
Windows and Android application binaries are outside this release.

Physical DAC behavior and signed-in external sources need testing on each
listener's own setup.
