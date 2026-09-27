import QtQuick
import com.blitzfc.chilpa
import "../Primitives"

// The three placements offered by every collection's queue button.
Menu {
    id: menu

    property TrackListModel model
    property string laterLabel
    property bool canRemove: false
    property int groupRow: -1
    property string activeLabel

    signal remove()

    function show(left, top) {
        groupRow = -1
        activeLabel = laterLabel
        showAt(left, top)
    }

    function showGroup(row, label, left, top) {
        groupRow = row
        activeLabel = label
        showAt(left, top)
    }

    function showAt(left, top) {
        entries = [
            { text: I18n.tr("Add to queue", I18n.rev), act: "queue" },
            { text: I18n.tr("Play next", I18n.rev), act: "next" },
            { text: I18n.tr("Play later", I18n.rev), act: "later" }
        ].concat(canRemove ? [
            { separator: true },
            { text: I18n.tr("Delete playlist", I18n.rev), act: "remove" }
        ] : [])
        x = left
        y = top
        open()
    }

    onChosen: (index) => {
        const action = entries[index].act
        if (groupRow >= 0)
            model.groupAction(groupRow, action, activeLabel)
        else if (action === "queue")
            model.queueAll()
        else if (action === "next")
            model.playNextAll()
        else if (action === "later")
            model.playLaterAll(activeLabel)
        else if (action === "remove")
            menu.remove()
    }
}
