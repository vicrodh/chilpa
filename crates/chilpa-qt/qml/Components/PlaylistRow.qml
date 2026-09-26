import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A playlist in the sidebar (design/03 PlaylistRow): its icon and name, the
// open one marked. While it is renamed the name is a field: Enter keeps the
// new one, Esc the old.
Item {
    id: row

    required property int index
    required property int playlistId
    required property string name
    required property int tracks
    property bool active: false
    property bool editing: false

    signal opened()
    signal menu(real x, real y)
    signal renamed(string name)

    implicitHeight: Metrics.playlistRowHeight

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusControl
        color: row.active ? Theme.color.stateSelected : hover.hovered ? Theme.color.stateHover : "transparent"
    }

    Icon {
        id: icon
        x: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.playlist
        size: Metrics.iconMd
        color: Theme.color.textSecondary
    }

    Text {
        visible: !row.editing
        anchors.left: icon.right
        anchors.leftMargin: Metrics.navItemPadding
        anchors.right: parent.right
        anchors.rightMargin: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideRight
        font.pointSize: Type.body
        font.weight: row.active ? Type.semibold : Type.regular
        color: Theme.color.textPrimary
    }

    TextField {
        id: field
        visible: row.editing
        anchors.left: icon.right
        anchors.leftMargin: Metrics.gap6
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: parent.height - Metrics.gap2 * 2 // derived: a hair inside the row
        onAccepted: {
            if (text.trim() !== "")
                row.renamed(text.trim())
            row.editing = false
        }
        Keys.onEscapePressed: row.editing = false
    }

    onEditingChanged: {
        if (editing) {
            field.text = name
            field.focusInput()
        }
    }

    HoverHandler { id: hover }
    TapHandler {
        enabled: !row.editing
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => button === Qt.RightButton ? row.menu(point.position.x, point.position.y) : row.opened()
        onDoubleTapped: row.editing = true
    }
}
