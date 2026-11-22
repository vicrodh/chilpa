# 0.1.3 - Genres & Filters

The biggest update since the first release: 480 changes since 0.1.2. Chilpa now browses a collection the way it is organized, by genre, by composer and by work, and it talks to your media servers in both directions.

## Highlights

  - Genres, Composers and Compositions. A genre tree of Chilpa's own, several levels deep, with an image for 1,182 genres. Compositions open on the classical repertoire with each work's composer, period and form, and a switch shows every work. A composer's page opens like an artist's, with a picture you can change.
  - Your media servers, both ways. Plex, Jellyfin and OpenSubsonic playlists show up and change from Chilpa, and favorites and star ratings stay in step with the server. DLNA/UPnP servers can be a library source.
  - Offline copies from your self-hosted servers. Make tracks, albums and playlists from a music server (not from plugins) available offline from their menus; each row shows its copy's state.
  - More scrobblers and a Recap. Listens go to Koito, Libre.fm and a GNU FM server of your own, and Stats has a Recap page and a mosaic of the year. By the way, Last.fm scrobbling is assured for at least a year, thanks to QBZ's sponsors.
  - Protect my library. One switch in Settings > Library, and Chilpa changes nothing in your music files or on your servers: no tag or artwork writes, no deletions, no server playlist changes. Edits can still go to sidecar files if you allow it.
  - Lots of quality-of-life improvements: more search options and a sidebar you can customize. The playback engine was hardened in depth to keep the bit-perfect promise.
  - General optimizations. Everything should feel more fluid, with CPU and memory use kept under control.
  - A self-hosted Flatpak. Flathub has tightened its rules for AI-generated content, so Chilpa will not be published on Flathub until it has been around for a few months and has an active community. Until then, Chilpa offers its own Flatpak repository.
  - A rebuilt DSP plugin. Rewritten from the ground up, more advanced and easier to use at the same time.
  - Chilpa Nest, a new experimental plugin. It puts your albums back on the wall shelves where they belong. I hope you like it, and I would love your feedback on it.
  - And a third-party plugin: "that" plugin you have been waiting for. ;)

Heads-up for 0.1.2 and earlier: four weeks after this release, Chilpa's own services (lyrics, Last.fm scrobbling, Discogs metadata, podcast search and playlist import) answer only signed official builds. Update to keep them working. Forks and builds of your own can run their own services: https://github.com/vicrodh/chilpa-api#forks

Thank you to everyone who opened issues, and to @jakumule (Nix flake, #32), @TheMelmacian (German and English translations, #35), @bruce-awareit (Traditional Chinese translation, #42) and @afonsojramos (Welcome deck fix).

---
