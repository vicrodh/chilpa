// The queue, with the playing track marked.
//
// A track that is not playing yet can be removed; the playing one cannot,
// that is what the next button is for. Removing it does not take it off its
// line: it is greyed and struck through, so the list does not jump under the
// view.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

Rectangle {
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"

    ListView {
        id: list
        anchors.fill: parent
        anchors.margins: 10
        clip: true
        model: Chilpa.queue
        currentIndex: Chilpa.queueIndex
        // As the queue advances, the playing track stays in view.
        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

        delegate: Rectangle {
            width: list.width
            height: 26
            radius: 6
            color: index === Chilpa.queueIndex
                ? "#f3e7e7"
                : (mouse.containsMouse ? "#f8f4f0" : "transparent")

            readonly property bool removed: Chilpa.queueRemoved[index] === "1"
            readonly property bool playing: index === Chilpa.queueIndex

            MouseArea {
                id: mouse
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }

            Label {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: 8
                anchors.right: remove.left
                anchors.rightMargin: 6
                elide: Text.ElideRight
                text: (index + 1) + ".  " + modelData
                font.strikeout: parent.removed
                opacity: parent.removed ? 0.45 : 1
                color: parent.playing ? "#d7272b" : "#411d1f"
            }

            Label {
                id: remove
                anchors.verticalCenter: parent.verticalCenter
                anchors.right: parent.right
                anchors.rightMargin: 8
                text: Chilpa.translate("remove", Chilpa.trRev)
                font.pixelSize: 11
                color: "#7a411d1f"
                visible: mouse.containsMouse && !parent.removed
                    && index > Chilpa.queueIndex
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Chilpa.removeFromQueue(index)
                }
            }
        }
    }
}
