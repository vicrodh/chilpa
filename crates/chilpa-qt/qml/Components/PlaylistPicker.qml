import QtQuick
import com.blitzfc.chilpa
import "../Primitives"

// Which playlist something goes to: a new one, or one of ours. It opens
// where the menu that asked was, which is closed by then.
Menu {
    id: picker

    property var ids: []

    // Zero for a new playlist.
    signal picked(int playlist)

    function show(left, top) {
        ids = Playlists.ids(Playlists.rev)
        const lists = Playlists.names(Playlists.rev).map(n => ({ text: n }))
        entries = [{ text: I18n.tr("New playlist", I18n.rev) }].concat(lists.length > 0 ? [{ separator: true }] : [], lists)
        x = left
        y = top
        open()
    }

    // The first is a new one; after the separator, ours in order.
    onChosen: (index) => picker.picked(index === 0 ? 0 : Number(ids[index - 2]))
}
