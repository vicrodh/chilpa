import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A sidebar entry (design/03 NavItem). Collapsed, only its icon shows, and
// the label moves to a tooltip.
Rectangle {
    id: item

    property string text
    property string glyph
    property bool tintIcon: false
    property url iconSource
    property bool active: false
    property bool collapsed: false
    property bool expandable: false
    property bool expanded: false
    signal expansionClicked()

    signal clicked()
    signal middleClicked()

    implicitHeight: Metrics.navItemHeight
    radius: Metrics.radiusControl
    color: active ? Theme.color.stateSelected : area.containsMouse ? Theme.color.stateHover : "transparent"

    Icon {
        id: icon
        objectName: "navGlyph"
        anchors.left: parent.left
        anchors.leftMargin: item.collapsed ? (item.width - width) / 2 : Metrics.navItemPadding // derived: centered when collapsed
        anchors.verticalCenter: parent.verticalCenter
        glyph: item.glyph
        size: Metrics.iconNav
        color: Theme.color.accent
        visible: picture.status !== Image.Ready
    }

    Image {
        id: picture
        objectName: "navImage"
        anchors.fill: icon
        source: item.iconSource
        sourceSize.width: Metrics.iconNav
        sourceSize.height: Metrics.iconNav
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        visible: status === Image.Ready && !item.tintIcon
    }

    MultiEffect {
        anchors.fill: picture
        source: picture
        colorization: 1
        colorizationColor: Theme.color.accent
        visible: item.tintIcon && picture.status === Image.Ready
    }

    Text {
        anchors.left: icon.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: parent.right
        anchors.rightMargin: item.expandable ? Metrics.navItemHeight : Metrics.gap8
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
    IconButton {
        objectName: "sourceCaret"
        visible: item.expandable && !item.collapsed
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        glyph: item.expanded ? Icons.chevronDown : Icons.chevronRight
        tip: item.expanded ? I18n.tr("Collapse", I18n.rev) : I18n.tr("Expand", I18n.rev)
        onClicked: item.expansionClicked()
    }
}
