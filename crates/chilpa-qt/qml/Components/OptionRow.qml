import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A switch with what it does (the player's options, QBZ's shape): the name,
// a line under it, the toggle on the right. Unavailable, it says why in that
// line and the toggle does not move.
Item {
    id: row

    property string title
    property string detail
    property bool checked: false
    property bool available: true

    signal toggled(bool on)

    implicitHeight: Math.max(Metrics.optionRow, texts.implicitHeight + Metrics.gap10)

    Column {
        id: texts
        anchors.left: parent.left
        anchors.right: toggle.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: row.title
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.medium
            color: row.available ? Theme.color.textPrimary : Theme.color.textTertiary
        }
        Text {
            width: parent.width
            visible: row.detail !== ""
            text: row.detail
            wrapMode: Text.Wrap
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }

    Toggle {
        id: toggle
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        enabled: row.available
        opacity: row.available ? 1 : Metrics.disabledOpacity
        checked: row.checked
        onToggled: (on) => row.toggled(on)
    }
}
