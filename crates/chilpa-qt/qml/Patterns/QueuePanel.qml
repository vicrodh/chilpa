import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The side queue (design/03, frontend ADR-0008): what plays, then what
// comes next.
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
        detail: queue.count > 1 ? I18n.trCount("{} next", "{} next", queue.count - 1, I18n.rev) : ""

        IconButton {
            tip: I18n.tr("Clear the queue", I18n.rev)
            glyph: Icons.trash
            size: Metrics.button26
            enabled: queue.count > 0
            onClicked: Player.clearQueue()
        }
        IconButton {
            tip: I18n.tr("Close", I18n.rev)
            glyph: Icons.x
            size: Metrics.button26
            onClicked: panel.closed()
        }
    }

    ListView {
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        model: queue
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        delegate: QueueRow {
            width: ListView.view.width - Metrics.gap8
            x: Metrics.gap4
            onPlay: queue.play(index)
            onRemove: Player.removeQueued(index)
        }
    }

    Text {
        visible: queue.count === 0
        anchors.centerIn: parent
        text: I18n.tr("Nothing queued.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
}
