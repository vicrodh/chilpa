import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The log viewer's button: a bordered field with a glyph and a label, 32
// high, as QBZ's `IconTextButton`. Danger paints the glyph and the label in
// the negative color.
Rectangle {
    id: root
    property string text
    property string glyph
    property string iconFamily
    property bool danger: false
    property real maximumWidth: Number.POSITIVE_INFINITY
    signal clicked()
    implicitWidth: label.implicitWidth + (icon.visible ? icon.width + Metrics.gap6 : 0) + Metrics.gap12 * 2 // derived: horizontal padding on both sides
    implicitHeight: Math.max(Metrics.button32,label.implicitHeight + Metrics.gap4 * 2) // derived: vertical padding above and below
    width: Math.min(implicitWidth,maximumWidth)
    radius: Metrics.radiusControl
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    color: area.containsMouse && root.enabled ? Theme.color.stateSelected : Theme.color.fillField
    opacity: root.enabled ? 1 : Metrics.disabledOpacity
    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap6
        Icon {
            id: icon
            anchors.verticalCenter: parent.verticalCenter
            visible: root.glyph !== ""
            glyph: root.glyph
            iconFamily: root.iconFamily
            size: Metrics.iconMd
            color: root.danger ? Theme.color.statusNegative : Theme.color.textSecondary
        }
        Text {
            id: label
            width: Math.max(0,root.width - Metrics.gap12 * 2 - (icon.visible ? icon.width + Metrics.gap6 : 0)) // derived: the label between its padding and optional icon
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            font.pointSize: Type.body
            font.weight: Type.medium
            color: root.danger ? Theme.color.statusNegative : Theme.color.textSecondary
        }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (root.enabled) root.clicked()
    }
}
