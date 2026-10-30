# Chilpa 0.1.1

This is my app. Use it, enjoy it, and tell me what you find: your feedback is how Chilpa grows from here.

Chilpa runs on Linux and macOS, on x86_64 and ARM64 (Apple Silicon).

- Local libraries and connected sources in one native Qt interface.
- Multiple queues, playlists, album editions, search and a keyboard command line.
- Synchronized lyrics and translations, with immersive audio visualizations.
- An official plugin catalog, configured on first start with an offline snapshot. Radio, Podcasts, Playlist Importer, DSP and Mood are separate official plugins.
- The AppImage, the Linux tarball and the macOS app update themselves, verifying each download against the release checksum before replacing the installed copy. DEB and RPM installs tell you when a new version exists and update through your package manager.
- Linux AppImage, tarball, DEB and RPM packages, and macOS ZIP and DMG packages, with checksums, provenance and third-party notices.

What changed since the 0.1.0 preview:

- Audio outputs are listed away from the window, so the window no longer freezes while a sound server answers. Settings > Audio fills in by itself when the list arrives.
- The theme follows the desktop on sessions where Qt reports none, such as Hyprland.
- Third-party plugins show who publishes them, and older versions of a plugin are one small link below Install.
- A plugin can name its music source apart from its package name.
- Edits to records from your own servers are kept on this device, over what the server says.
- A server sign-in could be lost at the next start on a fresh install. Fixed, and the log now says which step failed.
- A numbered setup wizard in source settings.
- Narrower scrollbars, and playlists indent correctly inside folders.
- `apt`, `dnf`, the AUR (`chilpa-bin`) and a Gentoo overlay now carry Chilpa.

Third-party playback plugins and model weights are not included. Mood downloads
optional models only after it is enabled; their separate license is documented
in the plugin package. Native plugin packages are selected by platform.
Windows and Android application binaries are not part of this release.

Physical DAC behavior and signed-in external sources need testing on each
listener's own setup, so that is where I most need your reports. Open an issue
for anything that breaks, anything that surprises you, or anything you wish
Chilpa did.
