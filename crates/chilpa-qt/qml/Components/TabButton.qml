import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A tab: icon, title and its own close button (design/02 3). A middle click
// closes it too.
Item {
    id: tab

    property string title
    property string glyph
    property bool active: false

    signal activated()
    signal closed()

    implicitWidth: Math.min(Metrics.tabMaxWidth, row.implicitWidth + Metrics.tabPadLeft + Metrics.tabPadRight)
    implicitHeight: Metrics.tabHeight
    RectangularShadow {
        anchors.fill: panel
        visible: tab.active
        radius: panel.radius
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowTabActive
    }

    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: tab.active ? Theme.color.surfaceRaised : area.containsMouse ? Theme.color.stateHover : "transparent"

        Edge { visible: tab.active }
    }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        onClicked: (mouse) => mouse.button === Qt.MiddleButton ? tab.closed() : tab.activated()
    }

    Row {
        id: row
        anchors.left: parent.left
        anchors.leftMargin: Metrics.tabPadLeft
        anchors.right: parent.right
        anchors.rightMargin: Metrics.tabPadRight
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: tab.glyph
            size: Metrics.iconSm
            color: Theme.color.textTertiary
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Metrics.tabMaxWidth - Metrics.tabPadLeft - Metrics.tabPadRight - Metrics.iconSm - Metrics.tabClose - Metrics.gap12)
            text: tab.title
            elide: Text.ElideRight
            font.pointSize: Type.meta
            font.weight: tab.active ? Type.semibold : Type.regular
            color: tab.active ? Theme.color.textPrimary : Theme.color.textSecondary
        }

        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Close tab", I18n.rev)
            glyph: Icons.x
            size: Metrics.tabClose
            iconSize: Metrics.iconXs
            onClicked: tab.closed()
        }
    }
}
