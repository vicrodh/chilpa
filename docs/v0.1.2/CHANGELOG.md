# Chilpa 0.1.2

### Día de muertos release

No features release, just bugfixing.

- Radio no longer asks for the song's cover on every audio write. While a station played, 0.1.1 sent hundreds of requests a minute to Chilpa's server, and most of them were refused. Now it asks once per song, the cover stays on screen, and it comes back after a reconnect.
- Native DSD through PipeWire now plays on a real DAC. In 0.1.1 every DSD track stopped half a second in with "Unexpected DSD format", and pause and seek failed the same way. It needs PipeWire 1.6 or later; with an older PipeWire, Chilpa converts DSD to PCM.
- On a fresh install, two parts of Chilpa starting at once could each create their own install id, and credentials saved with one of them could not be read afterwards. Fixed.

Physical DAC behavior still needs testing on each listener's own setup. Open an issue for anything that breaks.
