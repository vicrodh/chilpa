import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One base colour of the custom theme (QBZ's `TokenCell`): a swatch with
// its name under it. A click opens the shared picker under the grid, and a
// click on the open one closes it; Space and Enter do the same.
Column {
    id: cell

    property string label
    property color swatch
    property bool open: false

    signal activated()

    spacing: Metrics.gap6

    Rectangle {
        id: sample
        objectName: "colorSwatch"
        width: Editor.swatchLong
        height: Editor.swatchTall
        radius: Metrics.radiusCard
        border.width: cell.open || sample.activeFocus ? Metrics.focusRing : 1
        border.color: cell.open || sample.activeFocus ? Theme.color.accent : Theme.color.strokeHairline
        color: cell.swatch
        activeFocusOnTab: visible && enabled
        Accessible.role: Accessible.Button
        Accessible.name: cell.label
        Accessible.onPressAction: cell.activated()
        Keys.onPressed: (event) => {
            if (!event.isAutoRepeat && (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) {
                cell.activated()
                event.accepted = true
            }
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onPressed: sample.forceActiveFocus()
            onClicked: cell.activated()
        }
    }
    Text {
        width: Editor.swatchLong
        text: cell.label
        color: cell.open ? Theme.color.textPrimary : Theme.color.textTertiary
        font.pointSize: Type.small
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }
}
