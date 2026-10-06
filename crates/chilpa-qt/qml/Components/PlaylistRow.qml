import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

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
    // Paths of the covers of its first records, up to four.
    required property var covers
    // Its own picture when it has one on disk, its records' covers if not.
    readonly property var shown: row.artwork !== "" && row.artwork.indexOf("://") < 0 ? [row.artwork] : row.covers
    property bool active: false
    property bool editing: false
    property string sourceIcon: ""
    property bool showSourceBadge: false

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

    // The icon's place, as wide as the collage and centered where the icon
    // is, so the names stay in line with the folders'.
    Item {
        id: icon
        x: Metrics.navItemPadding + (row.folderId > 0 ? Metrics.playlistIndent : 0) - (width - Metrics.iconMd) / 2 // derived: centered on the icon
        width: Metrics.playlistCollage
        height: parent.height

        Icon {
            anchors.centerIn: parent
            visible: !collage.active
            glyph: row.local ? Icons.playlist : Icons.server
            size: Metrics.iconMd
            color: Theme.color.textSecondary
        }
        Loader {
            id: collage
            anchors.centerIn: parent
            active: SettingsModel.playlistCollage && row.shown.length > 0
            sourceComponent: PlaylistCollage { covers: row.shown; tint: row.index }
        }
    }

    SourceIcon {
        id: sourceBadge
        anchors.right: parent.right
        anchors.rightMargin: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        visible: row.showSourceBadge && !row.editing
        sourceInfo: ({icon: row.sourceIcon, label: row.sourceName})
    }
    Text {
        visible: !row.editing
        anchors.left: icon.right
        anchors.leftMargin: Metrics.navItemPadding - (icon.width - Metrics.iconMd) / 2 // derived: where the name was
        anchors.right: parent.right
        anchors.rightMargin: Metrics.navItemPadding + (sourceBadge.visible ? sourceBadge.width + Metrics.gap6 : 0)
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
