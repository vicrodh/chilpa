import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Parts
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The active queue drives local playback, casting and ListenList.
Item {
    id: panel

    signal closed()
    Timer { interval: Motion.listenRefresh; running: panel.visible; repeat: true; onTriggered: ListenList.refresh() }

    TrackListModel {
        id: queue
        kind: "queue"
        filterRev: Player.queueRev + ListenList.revision + Library.remoteRev
        playingPath: Player.path
    }

    PanelHeader {
        id: head
        width: parent.width
        title: I18n.tr("Queue", I18n.rev)
        detail: queue.summary

        IconButton {
            tip: I18n.tr("Open ListenList", I18n.rev)
            glyph: Icons.listDetails
            size: Metrics.button26
            onClicked: Navigation.go("listenlist", 0)
        }
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
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}

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

    Parts.QueueFooter {
        id: foot
        anchors.bottom: parent.bottom
        width: parent.width
        onClear: Player.clearQueue()
        sleepLeft: Controls.sleepLeft
        onTimer: sleep.open()
        onSearched: (text) => queue.key = text
        onSave: Playlists.saveQueue(I18n.trArgs("Queue · {}", [Qt.locale().toString(new Date(), Locale.ShortFormat)], I18n.rev))
    }

    Parts.SleepTimer { id: sleep; parent: foot; anchors.centerIn: undefined; x: foot.width - width - Metrics.gap6; y: -height - Metrics.gap6 }

    TrackMenu {
        id: menu
        model: queue
    }
}
