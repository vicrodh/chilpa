# 0.1.0-beta

I prepare the first public Chilpa beta for Linux x86_64.

- I browse local libraries and connected sources through one native Qt interface.
- I keep navigation responsive by presenting the destination skeleton before loading its content.
- I include multiple queues, playlists, album editions, search and a keyboard command line.
- I show synchronized lyrics and translations, with immersive audio visualizations.
- I configure the official plugin catalog on first start, including an offline snapshot.
- I offer Radio, Podcasts, Playlist Importer, DSP and Mood as separate official plugins.
- I package an AppImage and a binary tarball with checksums, provenance and third-party notices.
- I keep beta updates separate from stable installations and verify updates before replacing an AppImage.

I do not include third-party playback plugins or model weights. Mood downloads
optional models only after it is enabled; their separate license is documented
in the plugin package. I ship Linux x86_64 process plugins in this beta;
macOS, Windows and Android binaries remain outside this release.

I treat this as a beta. Physical DAC behavior and signed-in external sources
still need testing on each listener's own setup.
