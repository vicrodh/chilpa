import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.controls
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

// The A-B loop among the playback options: one button that marks where the
// stretch starts, then where it ends, and another that lets it go.
Item {
    id: row

    implicitHeight: Metrics.optionRow

    Text {
        anchors.left: parent.left
        anchors.right: buttons.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("A-B loop", I18n.rev)
        elide: Text.ElideRight
        font.pointSize: Type.body
        font.weight: Type.medium
        color: Theme.color.textPrimary
    }
    Row {
        id: buttons
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        PillButton {
            objectName: "loopMark"
            visible: Controls.loopState < 2
            text: Controls.loopState === 0 ? I18n.tr("Set loop start (A)", I18n.rev) : I18n.tr("Set loop end (B)", I18n.rev)
            onClicked: Controls.markLoop()
        }
        IconButton {
            objectName: "loopClear"
            visible: Controls.loopState > 0
            tip: I18n.tr("Clear loop", I18n.rev)
            glyph: Icons.x
            onClicked: Controls.clearLoop()
        }
    }
}
