import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One row of the immersive queue: 56 tall with a radius of 10, the cover
// at 40 inside a hairline, title and artist, and the duration that gives
// way to a round play button on hover. The row that plays is brighter and
// edged, and offers no button. A click plays it.
Rectangle {
    id: line
    property string title
    property string artist
    property string duration
    property url art
    property bool current: false
    signal play()
    height: Stage.queueRowTall
    radius: Stage.queueRowRound
    color: current ? Theme.color.immersiveQueueRow : (rowArea.containsMouse ? Theme.color.immersiveQueueRowHover : "transparent")
    border.width: current ? Metrics.hairline : 0
    border.color: Theme.color.immersiveQueueRowEdge
    Behavior on color { ColorAnimation { duration: Motion.reduced ? 0 : Stage.hoverFade } }
    MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: line.play() }
    Row {
        anchors.fill: parent
        anchors.leftMargin: Stage.menuRowLead
        anchors.rightMargin: Stage.queueRowPad
        spacing: Stage.menuRowPad
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: Stage.queueCover
            height: Stage.queueCover
            radius: Stage.queueCoverRound
            color: Theme.color.immersiveQueueCover
            border.width: Metrics.hairline
            border.color: Theme.color.immersiveQueueCoverEdge
            ArtTile { anchors.fill: parent; radius: Stage.queueCoverRound; edged: false; source: line.art }
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Stage.queueCover - Stage.menuRowPad * 2 - Stage.queuePlay // derived: what the cover and the trailing slot leave
            spacing: Stage.menuRowApart * 3 // derived: a hair between the lines
            Text { width: parent.width; text: line.title; color: Theme.color.immersiveTextStrong; font.pointSize: Stage.queueTitleText; font.weight: line.current ? Type.semibold : Type.medium; elide: Text.ElideRight }
            Text { width: parent.width; text: line.artist; color: Theme.color.immersiveTextMuted; font.pointSize: Stage.queueArtistText; elide: Text.ElideRight }
        }
        Item {
            anchors.verticalCenter: parent.verticalCenter
            width: Stage.queuePlay
            height: Stage.queuePlay
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: line.duration
                color: Theme.color.immersiveTextFaint
                font.pointSize: Stage.queueTimeText
                font.weight: Type.medium
                opacity: !line.current && rowArea.containsMouse ? 0 : 1
                Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.hoverFade } }
            }
            Rectangle {
                visible: !line.current
                anchors.fill: parent
                radius: width / 2 // derived: a circle
                color: playArea.containsMouse ? Theme.color.immersiveQueuePlayHover : Theme.color.immersiveQueuePlay
                opacity: rowArea.containsMouse || playArea.containsMouse ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.hoverFade } }
                Text {
                    anchors.centerIn: parent
                    anchors.horizontalCenterOffset: 1
                    text: Icons.playerPlayFilled
                    font.family: Icons.filled
                    font.pixelSize: Stage.ghostIcon
                    color: Theme.color.immersiveTextStrong
                }
                MouseArea { id: playArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: line.play() }
            }
        }
    }
}
