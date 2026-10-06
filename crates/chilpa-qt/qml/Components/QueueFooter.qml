import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

// The foot of the side queue (design/03 QueueFooter): empty it, keep it as
// a playlist and, on the right, the timer that stops playback, which says
// what is left while it runs.
Item {
    id: foot

    signal clear()
    signal save()
    signal timer()
    // What is left of the timer, or empty while it is off.
    property string sleepLeft

    implicitHeight: Metrics.panelHeader

    Rectangle {
        width: parent.width
        height: 1
        color: Theme.color.strokeHairline
    }

    Row {
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        PillButton {
            kind: "ghost"
            glyph: Icons.clearAll
            text: I18n.tr("Clear", I18n.rev)
            onClicked: foot.clear()
        }
        PillButton {
            kind: "ghost"
            glyph: Icons.playlistAdd
            text: I18n.tr("Save as playlist", I18n.rev)
            onClicked: foot.save()
        }
    }
    PillButton {
        objectName: "queueTimer"
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        kind: foot.sleepLeft !== "" ? "primary" : "ghost"
        text: foot.sleepLeft !== "" ? foot.sleepLeft : I18n.tr("Timer", I18n.rev)
        onClicked: foot.timer()
    }
}
