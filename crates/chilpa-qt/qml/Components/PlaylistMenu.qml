import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// What a playlist offers on a right click (design/04 9): play it, queue it,
// rename it, delete it. Deleting cannot be undone, so it asks first, in the
// same place.
Menu {
    id: menu

    property int playlistId: 0
    property string name
    property bool confirming: false

    signal rename()

    function show(id, title, left, top) {
        playlistId = id
        name = title
        menu.title = title
        confirming = false
        x = left
        y = top
        open()
    }

    entries: confirming ? [
        { text: I18n.trArgs("Delete “{}”?", [menu.name], I18n.rev), note: true },
        { text: I18n.tr("Delete", I18n.rev) },
        { text: I18n.tr("Cancel", I18n.rev) }
    ] : [
        { text: I18n.tr("Play now", I18n.rev), glyph: Icons.playerPlayFilled, filled: true },
        { text: I18n.tr("Shuffle", I18n.rev) },
        { text: I18n.tr("Add to queue", I18n.rev), glyph: Lucide.listEnd, family: Lucide.font },
        { separator: true },
        { text: I18n.tr("Rename", I18n.rev) },
        { text: I18n.tr("Delete…", I18n.rev) }
    ]

    onChosen: (index) => {
        if (confirming) {
            if (index === 1)
                Playlists.remove(playlistId)
            return
        }
        switch (index) {
        case 0: Playlists.play(playlistId, false); break
        case 1: Playlists.play(playlistId, true); break
        case 2: Playlists.queue(playlistId); break
        case 4: menu.rename(); break
        case 5:
            confirming = true
            open()
            break
        }
    }
}
