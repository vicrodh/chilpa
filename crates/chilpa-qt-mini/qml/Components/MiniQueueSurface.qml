import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Surface 3 (02-mini §1.3): what plays and the whole of what comes after
// it, as the queue model already projects it, in rows that mirror QBZ's
// and stop there: a position number where a cover would go, no hover
// actions, no menu, no reordering. Hover paints before the playing row's
// tint, as in QBZ. A click on the playing row resumes it if it is paused
// and never restarts it; a click on any other row plays that one.
Item {
    id: surface

    TrackListModel {
        id: queue
        kind: "queue"
        filterRev: Player.queueRev + ListenList.revision
        playingPath: Player.path
    }
    Timer { interval: Motion.listenRefresh; running: surface.visible; repeat: true; onTriggered: ListenList.refresh() }

    Text {
        anchors.fill: parent
        visible: queue.count === 0
        text: I18n.tr("Queue is empty", I18n.rev)
        color: Theme.color.textTertiary
        font.pointSize: Card.emptyText
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }

    ListView {
        id: list
        objectName: "miniQueue"
        anchors.fill: parent
        visible: queue.count > 0
        topMargin: Card.queueTop
        bottomMargin: Card.queueBottom
        spacing: 0
        cacheBuffer: Card.queueCache
        reuseItems: true
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: queue
        ScrollBar.vertical: ViewportScrollBar {}

        delegate: Rectangle {
            id: row

            required property int index
            required property string title
            required property string artist
            required property string duration
            required property bool playing

            width: list.width
            height: Card.queueRow
            radius: Card.queueRowRound
            antialiasing: true
            color: area.containsMouse ? Theme.color.stateHover : row.playing ? Theme.color.miniRowActive : "transparent"

            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    if (!row.playing)
                        queue.play(row.index)
                    else if (!Player.playing || Player.paused)
                        Player.toggle()
                }
            }
            Rectangle {
                visible: row.playing
                x: 0
                y: Card.queueBarIn
                width: Card.queueBar
                height: row.height - Card.queueBarIn * 2 // derived: inset above and below
                radius: Card.queueBar
                color: Theme.color.accent
            }
            Text {
                id: number
                x: Card.queueNumberX
                width: Card.queueNumberSpan
                anchors.verticalCenter: parent.verticalCenter
                text: String(row.index + 1)
                color: row.playing ? Theme.color.accent : Theme.color.textTertiary
                font.pointSize: Card.bodyText
                horizontalAlignment: Text.AlignHCenter
            }
            Text {
                id: length
                anchors.right: parent.right
                anchors.rightMargin: Card.queueSidePad
                anchors.verticalCenter: parent.verticalCenter
                text: row.duration
                color: Theme.color.textTertiary
                font.pointSize: Card.metaText
            }
            Column {
                anchors.left: number.right
                anchors.leftMargin: Card.queueApart
                anchors.right: length.left
                anchors.rightMargin: Card.queueApart
                anchors.verticalCenter: parent.verticalCenter
                spacing: Card.metaApart

                Text {
                    width: parent.width
                    text: row.title
                    color: row.playing ? Theme.color.textPrimary : Theme.color.textSecondary
                    font.pointSize: Card.bodyText
                    font.weight: Type.semibold
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    visible: row.artist !== ""
                    text: row.artist
                    color: Theme.color.textTertiary
                    font.pointSize: Card.smallText
                    elide: Text.ElideRight
                }
            }
        }
    }
}
