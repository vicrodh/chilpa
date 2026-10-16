import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// One tab of the immersive queue, Queue or History: its label at 18 with
// the count beside it on the same baseline, bold and underlined 2 px while
// active, lifted on hover.
Item {
    id: tab
    property string label
    property int count: 0
    property bool active: false
    signal clicked()
    width: count.visible ? count.x + count.width : label.width
    height: Stage.queueTabTall
    Item {
        id: row
        width: parent.width
        height: parent.height
        Text {
            id: label
            text: tab.label
            color: tab.active ? Theme.color.immersiveTextStrong : (area.containsMouse ? Theme.color.immersiveText : Theme.color.immersiveTextFaint)
            font.pointSize: Stage.queueTabText
            font.weight: tab.active ? Type.bold : Type.medium
            Behavior on color { ColorAnimation { duration: Motion.reduced ? 0 : Stage.hoverFade } }
        }
        Text {
            id: count
            visible: tab.count > 0
            x: label.width + Stage.queueTabApart / 4 // derived: a small gap before the count
            anchors.baseline: label.baseline
            text: "(" + tab.count + ")"
            color: tab.active ? Theme.color.immersiveTextMuted : Theme.color.immersiveTextQuiet
            font.pointSize: Stage.queueArtistText
            font.weight: Type.medium
            Behavior on color { ColorAnimation { duration: Motion.reduced ? 0 : Stage.hoverFade } }
        }
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: Stage.queueUnderline
        radius: Stage.tinyRound
        color: tab.active ? Theme.color.immersiveTextStrong : (area.containsMouse ? Theme.color.immersiveVolumeTrack : "transparent")
        Behavior on color { ColorAnimation { duration: Motion.reduced ? 0 : Stage.hoverFade } }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: tab.clicked() }
}
