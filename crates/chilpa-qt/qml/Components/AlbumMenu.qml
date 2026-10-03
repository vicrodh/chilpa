import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../Primitives"
import "../Theme"

// What a record offers from its menu (design/04, Album): play it or queue
// it -- or the whole selection when it is part of one -- add it to a
// playlist, open it here or in a new tab, its quick view where a grid can
// open one, its artist unless that is the page it is on, and its favorite.
Menu {
    id: menu

    property int albumId
    property string artist
    property int at: -1
    property bool canExpand: false
    // The artist whose page this is, so "Go to artist" leads nowhere new.
    property string artistPage
    property var actions: []
    property var nativeInfo: ({})

    signal quick(int at)

    function show(id, albumTitle, name, index, left, top) {
        albumId = id
        artist = name
        at = index
        const one = !(AlbumActions.selected(id, AlbumActions.rev) && AlbumActions.selectedCount > 1)
        title = one ? albumTitle : ""
        nativeInfo = id < 0 ? JSON.parse(Sources.card(id, Library.remoteRev) || "{}") : ({})
        const favorite = nativeInfo.plugin ? nativeInfo.favorite : Ratings.favorite("album", id, Ratings.rev)
        const all = [
            { text: I18n.trCount("{} record", "{} records", AlbumActions.selectedCount, I18n.rev), note: true, when: !one },
            { text: I18n.tr("Play now", I18n.rev), act: "play", glyph: Icons.playerPlayFilled, filled: true },
            { text: I18n.tr("Play next", I18n.rev), act: "next", glyph: Lucide.listStart, family: Lucide.font },
            { text: I18n.tr("Add to queue", I18n.rev), act: "queue", glyph: Lucide.listEnd, family: Lucide.font },
            // A source's record (a negative id) is not the library's to edit or mark.
            { text: I18n.tr("Add to playlist…", I18n.rev), act: "pick", glyph: Icons.playlistAdd, when: id > 0 },
            { separator: true, when: one },
            { text: I18n.tr("Open album", I18n.rev), act: "open", when: one },
            { text: I18n.tr("Open in new tab", I18n.rev), act: "tab", when: one },
            { text: I18n.tr("Quick view", I18n.rev), act: "quick", when: one && canExpand },
            { text: I18n.tr("Go to artist", I18n.rev), act: "artist", when: one && artist !== "" && artist !== artistPage },
            { text: I18n.tr("Edit metadata…", I18n.rev), act: "metadata", when: one && id > 0 },
            { separator: true, when: one && id > 0 },
            { text: favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev),
              act: favorite ? "unfavorite" : "favorite", when: one && (id > 0 || nativeInfo.plugin && /^(album|artist|label|playlist):/.test(nativeInfo.ref || "")) }
        ]
        actions = all.filter(e => e.when === undefined || e.when)
        entries = actions
        x = left
        y = top
        open()
    }

    onChosen: (index) => {
        switch (actions[index].act) {
        case "play": AlbumActions.play(albumId, false); break
        case "next": AlbumActions.playNext(albumId); break
        case "queue": AlbumActions.queue(albumId); break
        case "pick": picker.trackIdsJson = AlbumActions.trackIdsJson(albumId); picker.show(x, y); break
        case "open": Navigation.go("album", albumId); break
        case "tab": Navigation.open("album", albumId); break
        case "quick": menu.quick(at); break
        case "artist": { const key = Sources.artistKey(albumId, ""); if (key) Navigation.goKey("source", key); else Navigation.openArtist(artist); break }
        case "metadata": Navigation.go("metadata", albumId); break
        case "favorite":
        case "unfavorite":
            if (nativeInfo.plugin) Sources.command("native", nativeInfo.plugin, JSON.stringify({action:"favorite",ref:nativeInfo.ref,on:actions[index].act === "favorite"}))
            else Ratings.setFavorite("album", albumId, actions[index].act === "favorite")
            break
        }
    }

    PlaylistPicker {
        id: picker

        parent: menu.parent
    }
}
