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
    signal clicked()
    implicitWidth: row.implicitWidth + Metrics.gap12 * 2 // derived: padding on both sides
    implicitHeight: Metrics.button32
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
            anchors.verticalCenter: parent.verticalCenter
            visible: root.glyph !== ""
            glyph: root.glyph
            iconFamily: root.iconFamily
            size: Metrics.iconMd
            color: root.danger ? Theme.color.statusNegative : Theme.color.textSecondary
        }
        Text {
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
