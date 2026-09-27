import QtQuick
import "../Theme"
import "../Primitives"

// A sidebar entry (design/03 NavItem). Collapsed, only its icon shows, and
// the label moves to a tooltip.
Rectangle {
    id: item

    property string text
    property string glyph
    property bool active: false
    property bool collapsed: false

    signal clicked()
    signal middleClicked()

    implicitHeight: Metrics.navItemHeight
    radius: Metrics.radiusControl
    color: active ? Theme.color.stateSelected : area.containsMouse ? Theme.color.stateHover : "transparent"

    Icon {
        id: icon
        anchors.left: parent.left
        anchors.leftMargin: item.collapsed ? (item.width - width) / 2 : Metrics.navItemPadding // derived: centered when collapsed
        anchors.verticalCenter: parent.verticalCenter
        glyph: item.glyph
        size: Metrics.iconNav
        color: Theme.color.accent
    }

    Text {
        anchors.left: icon.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: !item.collapsed
        text: item.text
        elide: Text.ElideRight
        font.pointSize: Type.body
        font.weight: item.active ? Type.semibold : Type.regular
        color: Theme.color.textPrimary
    }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        onClicked: (mouse) => mouse.button === Qt.MiddleButton ? item.middleClicked() : item.clicked()
        onContainsMouseChanged: item.collapsed && containsMouse ? TipState.show(item.text, item) : TipState.hide(item)
    }
}
