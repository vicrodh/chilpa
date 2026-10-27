import QtQuick
import app.chilpa
import app.chilpa.devices
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// What a record offers from its menu (design/04, Album): play it or queue
// it, now, next, later or in an extra queue -- or the whole selection when
// it is part of one -- add it to a playlist, open it here or in a new tab,
// its quick view where a grid can open one, its artist unless that is the
// page it is on, whether it groups with the other versions of its name,
// and its favorite.
CascadingMenu {
    id: menu

    property int albumId
    property bool canExpand: false
    // The artist whose page this is, so "Go to artist" leads nowhere new.
    property string artistPage
    property var nativeInfo: ({})

    signal quick(int at)

    function show(id, albumTitle, name, index, left, top) {
        albumId = id
        const one = !(AlbumActions.selected(id, AlbumActions.rev) && AlbumActions.selectedCount > 1)
        title = one ? albumTitle : ""
        nativeInfo = id < 0 ? JSON.parse(Sources.card(id, Library.remoteRev) || "{}") : ({})
        const favorite = nativeInfo.plugin ? nativeInfo.favorite : Ratings.favorite("album", id, Ratings.rev)
        // 1 kept apart from its other versions, 0 grouped with one, -1 neither.
        const apart = one && id > 0 ? Library.albumApart(id) : -1
        const all = [
            { text: I18n.trCount("{} record", "{} records", AlbumActions.selectedCount, I18n.rev), note: true, when: !one },
            { text: I18n.tr("Play now", I18n.rev), act: "play", glyph: Icons.playerPlayFilled, filled: true },
            { text: I18n.tr("Add to queue", I18n.rev), act: "queue", glyph: Lucide.listEnd, family: Lucide.font },
            { text: I18n.tr("Play next", I18n.rev), act: "next", glyph: Lucide.listStart, family: Lucide.font },
            { text: I18n.tr("Play later", I18n.rev), act: "later", glyph: Lucide.listPlus, family: Lucide.font },
            // A source's record (a negative id) is not the library's to edit or mark.
            { text: I18n.tr("Add to playlist…", I18n.rev), act: "pick", glyph: Icons.playlistAdd, when: id > 0 },
            { text: I18n.tr("Sync to device…", I18n.rev), act: "sync", glyph: Icons.deviceMobile, when: id > 0 },
            { separator: true, when: one },
            { text: I18n.tr("Open album", I18n.rev), act: "open", when: one },
            { text: I18n.tr("Open in new tab", I18n.rev), act: "tab", when: one },
            { text: I18n.tr("Quick view", I18n.rev), act: "quick", when: one && canExpand, at: index },
            { text: I18n.tr("Go to artist", I18n.rev), act: "artist", when: one && name !== "" && name !== artistPage, artist: name },
            { text: I18n.tr("Edit metadata…", I18n.rev), act: "metadata", when: one && id > 0 },
            { text: apart === 1 ? I18n.tr("Group with other versions", I18n.rev) : I18n.tr("Don't group with other versions", I18n.rev),
              act: "apart", when: apart >= 0, keepApart: apart === 0 },
            { separator: true, when: one && id > 0 },
            { text: favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev),
              act: favorite ? "unfavorite" : "favorite", when: one && (id > 0 || nativeInfo.plugin && /^(album|artist|label|playlist):/.test(nativeInfo.ref || "")) }
        ]
        // The extra queues come from the facade, as they do for a track.
        entries = all.filter(e => e.when === undefined || e.when).concat(JSON.parse(AlbumActions.listenTargetsJson()))
        x = left
        y = top
        open()
    }

    onActionChosen: (entry) => {
        const act = entry.act || ""
        if (act.startsWith("buffer:")) { AlbumActions.listAction(albumId, act); ListenList.refresh(); if (act === "buffer:new") Navigation.go("listenlist", 0); return }
        switch (act) {
        case "play": AlbumActions.play(albumId, false); break
        case "next": AlbumActions.playNext(albumId); break
        case "later": AlbumActions.playLater(albumId, title); break
        case "queue": AlbumActions.queue(albumId); break
        case "pick": picker.trackIdsJson = AlbumActions.trackIdsJson(albumId); picker.show(x, y); break
        case "sync": if (DeviceSync.useTracks(AlbumActions.deviceTracksJson(albumId))) Navigation.go("deviceSync", 0); break
        case "open": Navigation.go("album", albumId); break
        case "tab": Navigation.open("album", albumId); break
        case "quick": menu.quick(entry.at); break
        case "artist": { const key = Sources.artistKey(albumId, ""); if (key) Navigation.goKey("source", key); else Navigation.openArtist(entry.artist); break }
        case "metadata": Navigation.go("metadata", albumId); break
        case "apart": Library.keepAlbumApart(albumId, entry.keepApart); break
        case "favorite":
        case "unfavorite":
            if (nativeInfo.plugin) Sources.command("native", nativeInfo.plugin, JSON.stringify({action:"favorite",ref:nativeInfo.ref,on:act === "favorite"}))
            else Ratings.setFavorite("album", albumId, act === "favorite")
            break
        }
    }

    PlaylistPicker {
        id: picker

        parent: menu.parent
    }
}
