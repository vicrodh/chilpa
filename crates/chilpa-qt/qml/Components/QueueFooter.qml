import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The foot of the side queue (design/03 QueueFooter): empty it, or keep it
// as a playlist. The timer the design puts on the right comes when the
// facade and the player have one; a button that did nothing would be worse
// than none.
Item {
    id: foot

    signal clear()
    signal save()

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
}
