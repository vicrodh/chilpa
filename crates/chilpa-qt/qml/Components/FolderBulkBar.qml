import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

// What can be done with the folders chosen in the tree: how many they are,
// queue them next, later or at the end, and let them go.
Item {
    id: bar

    property int count: 0

    signal acted(string action)
    signal cleared()

    implicitHeight: Metrics.button32

    Text {
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("{} sel", I18n.rev).replace("{}", bar.count)
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        IconButton { tip: I18n.tr("Play next", I18n.rev); glyph: Lucide.listStart; iconFamily: Lucide.font; onClicked: bar.acted("next") }
        IconButton { tip: I18n.tr("Play later", I18n.rev); glyph: Lucide.listPlus; iconFamily: Lucide.font; onClicked: bar.acted("later") }
        IconButton { tip: I18n.tr("Add to queue", I18n.rev); glyph: Lucide.listEnd; iconFamily: Lucide.font; onClicked: bar.acted("queue") }
        IconButton { tip: I18n.tr("Clear", I18n.rev); glyph: Icons.x; onClicked: bar.cleared() }
    }
}
