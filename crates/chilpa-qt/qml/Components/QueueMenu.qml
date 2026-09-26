import QtQuick
import com.blitzfc.chilpa
import "../Primitives"

// The three placements offered by every collection's queue button.
Menu {
    id: menu

    property TrackListModel model
    property string laterLabel

    function show(left, top) {
        entries = [
            { text: I18n.tr("Add to queue", I18n.rev), act: "queue" },
            { text: I18n.tr("Play next", I18n.rev), act: "next" },
            { text: I18n.tr("Play later", I18n.rev), act: "later" }
        ]
        x = left
        y = top
        open()
    }

    onChosen: (index) => {
        const action = entries[index].act
        if (action === "queue")
            model.queueAll()
        else if (action === "next")
            model.playNextAll()
        else
            model.playLaterAll(laterLabel)
    }
}
