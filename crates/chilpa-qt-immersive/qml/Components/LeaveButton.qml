import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import "../Theme"

// Back to the window from the view's bottom left corner, level with the
// player bar: a glass disc of the header capsule's size with a chevron
// down, quiet until the pointer is on it.
Rectangle {
    id: leave
    property string glyph: Icons.chevronDown
    property string iconFamily: Icons.outline
    property string tip: I18n.tr("Exit Immersive", I18n.rev)
    signal clicked()
    width: Stage.capsuleIdle
    height: Stage.capsuleIdle
    radius: width / 2 // derived: a circle
    color: area.containsMouse ? Theme.color.immersiveNav : Theme.color.immersiveGlass
    border.width: Metrics.hairline
    border.color: Theme.color.immersiveGlassEdge
    Accessible.role: Accessible.Button
    Accessible.name: tip

    Text {
        anchors.centerIn: parent
        text: leave.glyph
        font.family: leave.iconFamily
        font.pixelSize: Stage.capsuleIcon
        color: Theme.color.immersiveTextStrong
        opacity: area.containsMouse ? 1 : Stage.idleGlyph
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: leave.clicked()
        onContainsMouseChanged: containsMouse ? Kit.TipState.show(leave.Accessible.name, leave) : Kit.TipState.hide(leave)
    }
    onEnabledChanged: if (!enabled) Kit.TipState.hide(leave)
    onVisibleChanged: if (!visible) Kit.TipState.hide(leave)
    Component.onDestruction: Kit.TipState.hide(leave)
}
