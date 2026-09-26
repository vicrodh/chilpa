import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The side queue (design/03, frontend ADR-0008, wireframe 17): its head
// with how much is queued, "Now playing" and "Up next" over their rows, and
// a foot that clears it or keeps it as a playlist. Docking it below, the
// full queue, "Play later" groups, the timer and dragging to reorder are
// not in the player yet, so they have no buttons here.
Item {
    id: panel

    signal closed()

    TrackListModel {
        id: queue
        kind: "queue"
        filterRev: Player.queueRev
        playingPath: Player.path
    }

    PanelHeader {
        id: head
        width: parent.width
        title: I18n.tr("Queue", I18n.rev)
        detail: queue.summary

        IconButton {
            tip: I18n.tr("Close", I18n.rev)
            glyph: Icons.x
            size: Metrics.button26
            onClicked: panel.closed()
        }
    }

    ListView {
        id: list
        anchors.top: head.bottom
        anchors.bottom: foot.top
        width: parent.width
        clip: true
        model: queue
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        // The playing row comes first, so its section is the first one.
        section.property: "playing"
        section.delegate: Text {
            required property string section
            width: ListView.view.width
            height: Metrics.queueSection
            leftPadding: Metrics.gap12
            verticalAlignment: Text.AlignVCenter
            text: section === "true" ? I18n.tr("Now playing", I18n.rev) : I18n.tr("Up next", I18n.rev)
            font.pointSize: Type.label
            color: Theme.color.textTertiary
        }

        delegate: QueueRow {
            width: ListView.view.width - Metrics.gap8
            x: Metrics.gap4
            onPlay: queue.play(index)
            onMenu: (x, y) => {
                const p = mapToItem(panel, x, y)
                menu.show(index, p.x, p.y)
            }
        }
    }

    Text {
        visible: queue.count === 0
        anchors.centerIn: list
        text: I18n.tr("Nothing queued.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }

    QueueFooter {
        id: foot
        anchors.bottom: parent.bottom
        width: parent.width
        onClear: Player.clearQueue()
        onSave: Playlists.saveQueue(I18n.trArgs("Queue · {}", [Qt.locale().toString(new Date(), Locale.ShortFormat)], I18n.rev))
    }

    TrackMenu {
        id: menu
        model: queue
    }
}
