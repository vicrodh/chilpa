# Chilpa 0.1.3

## Explore your music

  - Genres: a genre tree of Chilpa's own, several levels deep, where a style can sit under more than one parent. Each genre page has an overview, its albums, artists, tracks and subgenres, and a search that reaches any style directly. Your own genre tags stay reachable even when the tree does not know them.
  - An image for 1,182 genres, loaded only when a card is on screen. A genre without one is a flat tile with its name, and you can choose your own card and header images.
  - Composers: every composer credited in your library, with a portrait you can change from the card's right-click menu. A composer's page opens like an artist's, with the years and period Chilpa knows.
  - Compositions: works with their composer, period and form, and the number of recordings in your library. They open on the classical repertoire; a switch shows every work. A work opens on its recordings, grouped by album and performer, and a recording on its parts.
  - A work is read from its work tag, or from the title of its parts ("Cantata, JLB 5: I. Chorus") when the track has a composer, on files and on source albums alike.

## Media servers

  - Plex, Jellyfin and OpenSubsonic: their playlists show up in Chilpa, kept as the server has them, and change through the server.
  - Favorites and star ratings stay in step with the servers that keep them, in both directions.
  - Edits to records, tracks and artists on Plex and Jellyfin are saved to the server; Chilpa's own copy only when you choose it.
  - DLNA/UPnP media servers can be a library source, found on the network or by address.
  - A step-by-step setup for a music server, which says how the first import will go before the server is added.
  - Make tracks, albums and playlists from a self-hosted music server (not from plugins) available offline from their menus. Each row shows its copy's state, and the Offline music page lists them.

## Listening

  - Scrobbling to Koito, Libre.fm and a GNU FM server of your own.
  - Stats has a Recap page, and the chart can switch to a mosaic of the year, one square a day.
  - Lyrics can show pronunciation for scripts you do not read. Off by default.
  - Discord shows the cover of what plays, uploaded only if you allow it, with play and pause badges.

## Library

  - Protect my library (Settings > Library): one switch, and Chilpa changes nothing in your music files or on your servers. Tag and artwork writes, deletions, server playlist changes and syncs into a library folder are refused, and the window greys them out. Edits can still go to sidecar files if you allow it.
  - Tracks: filter columns on their own, choose which columns show, and filter by bit depth and sample rate.
  - Track information in sections, for any track, from its menu or from the player's title.
  - Star ratings can be turned off.
  - Reorder and hide the sidebar entries in Settings. Playlists stay reachable with the sidebar collapsed, and Chilpa opens on the first entry when Home is hidden.
  - Albums a source marks as AI-generated say so.
  - Large libraries scroll and open faster, and covers on screen load first.
  - When a new version changes the library, the upgrade runs under the startup screen, keeps a copy, and puts it back if the upgrade fails.

## Playback

  - A track that cannot open or breaks partway is skipped and the queue goes on.
  - The player clears when a queue plays to its end.
  - Back restarts the track; a second press goes to the previous one.
  - Lock output device (Settings > Audio) keeps every play on the output you chose.
  - Casting finds any kind of UPnP renderer and remembers the ones it saw.
  - Casting to DLNA renderers now runs on QBZ's media server and control, and each track's address names its format. Marantz and other renderers that refused Chilpa's streams are asked to test it (#48).
  - Tracks of a SACD image and DSDIFF files can be cast: they go out as DSF, the same DSD bits in a container more renderers read.
  - A volume key shows the level for two seconds.

## Metadata

  - Edit chosen fields of several folders in one batch.
  - beets as an optional lookup engine in the metadata editor, with its Python environment managed in Settings.

## Plugins, updates and packages

  - Plugins can add full-window views and players, once you approve them.
  - Source sign-in windows work on macOS.
  - The update notice names the running and the offered version and no longer sits over the player bar. Restart Chilpa now restarts it even with close to tray on.
  - A Flatpak from Chilpa's own repository. Flathub has tightened its rules for AI-generated content, so Chilpa will not be on Flathub until it has been around for a few months and has an active community. Inside Flatpak, updates come from Flatpak, not from Chilpa.
  - The Linux AppImage carries the audio runtime it needs.
  - A Nix flake to build and develop Chilpa, by @jakumule (#32).
  - German and English translation fixes by @TheMelmacian (#35), and a Traditional Chinese translation by @bruce-awareit (#42).
  - A Welcome card's button can set the deck aside to finish later, by @afonsojramos.
  - About credits each contributor by GitHub username, with a link to their profile.

Physical DAC behavior and signed-in sources still need testing on each listener's own setup. Open an issue for anything that breaks.
