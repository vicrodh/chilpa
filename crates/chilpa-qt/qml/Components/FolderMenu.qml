import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// What can be done with a folder from where it is listed: it plays or is
// queued with everything under it.
Menu {
    id: menu

    property FolderModel folder
    property string path

    function show(of, left, top) {
        path = of
        x = left
        y = top
        open()
    }

    entries: [
        { act: "play", text: I18n.tr("Play", I18n.rev), glyph: Icons.playerPlayFilled, filled: true },
        { act: "next", text: I18n.tr("Play next", I18n.rev), glyph: Lucide.listStart, family: Lucide.font },
        { act: "later", text: I18n.tr("Play later", I18n.rev), glyph: Lucide.listPlus, family: Lucide.font },
        { act: "queue", text: I18n.tr("Add to queue", I18n.rev), glyph: Lucide.listEnd, family: Lucide.font }
    ]
    onChosen: (index) => folder.act(path, entries[index].act)
}
