# Chilpa 0.1.0

This is my app. Use it, enjoy it, and tell me what you find: your feedback is how Chilpa grows from here.

Chilpa runs on Linux and macOS, on x86_64 and ARM64 (Apple Silicon).

- Local libraries and connected sources in one native Qt interface.
- Multiple queues, playlists, album editions, search and a keyboard command line.
- Synchronized lyrics and translations, with immersive audio visualizations.
- An official plugin catalog, configured on first start with an offline snapshot. Radio, Podcasts, Playlist Importer, DSP and Mood are separate official plugins.
- The AppImage, the Linux tarball and the macOS app update themselves, verifying each download against the release checksum before replacing the installed copy. DEB and RPM installs tell you when a new version exists and update through your package manager.
- Linux AppImage, tarball, DEB and RPM packages, and macOS ZIP and DMG packages, with checksums, provenance and third-party notices.

Third-party playback plugins and model weights are not included. Mood downloads
optional models only after it is enabled; their separate license is documented
in the plugin package. Native plugin packages are selected by platform.
Windows and Android application binaries are not part of this release.

Physical DAC behavior and signed-in external sources need testing on each
listener's own setup, so that is where I most need your reports. Open an issue
for anything that breaks, anything that surprises you, or anything you wish
Chilpa did.
