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
    required property string source
    required property string sourceName
    required property bool local
    required property string artwork
    required property int folderId
    required property bool visibleInSidebar
    required property url cover1
    required property url cover2
    required property url cover3
    required property url cover4
    property bool active: false
    property bool editing: false

    signal opened()
    signal menu(real x, real y)
    signal renamed(string name)

    implicitHeight: Metrics.playlistRowHeight

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        radius: Metrics.radiusControl
        selected: row.active
        hovered: hover.hovered
    }

    Icon {
        id: icon
        x: Metrics.navItemPadding + (row.folderId > 0 ? Metrics.playlistIndent : 0)
        anchors.verticalCenter: parent.verticalCenter
        visible: !covers.active
        glyph: row.local ? Icons.playlist : Icons.server
        size: Metrics.iconMd
        color: Theme.color.textSecondary
    }

    Loader {
        id: covers
        x: Metrics.navItemPadding + (row.folderId > 0 ? Metrics.playlistIndent : 0)
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.iconMd
        height: Metrics.iconMd
        active: SettingsModel.playlistCollage && row.cover1.toString() !== ""

        sourceComponent: Grid {
            columns: 2
            Repeater {
                model: [row.cover1, row.cover2, row.cover3, row.cover4]
                ArtTile {
                    required property var modelData
                    required property int index
                    width: Metrics.playlistCollageTile
                    height: width
                    radius: 0
                    source: modelData
                    tint: index
                }
            }
        }
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
    HoverHandler { enabled: !row.editing; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        enabled: !row.editing
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => button === Qt.RightButton ? row.menu(point.position.x, point.position.y) : row.opened()
        onDoubleTapped: if (row.local) row.editing = true
    }
}
