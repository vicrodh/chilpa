import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One folder or playlist in the manager's list: its cover or folder, its
// name, whose it is and how much it holds, and the order's arrows and its
// menu at the end. The grid draws the same rows as cards (PlaylistCard).
Rectangle {
    id: row

    required property int index
    required property int rowKind
    required property int entryId
    required property string name
    required property string artwork
    required property string colorName
    required property int tracks
    required property bool local
    required property string source
    required property string sourceName

    property var manager
    // The sources' panels, for a source playlist's name.
    property var panels: []
    readonly property string owner: local ? "Chilpa" : (panels.find(panel => panel.id === source) || {}).name || sourceName
    property bool custom: false
    signal menuAsked(real x, real y)

    readonly property bool folder: rowKind === 1

    implicitHeight: Metrics.playlistManagerListRow
    radius: Metrics.radiusCard
    color: "transparent"

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        listMode: !row.folder
        hovered: hover.hovered
        restColor: "transparent"
        radius: row.radius
    }

    Icon {
        visible: row.folder
        x: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.folder
        size: Metrics.iconMd
        color: row.manager ? row.manager.folderColor(row.colorName) : Theme.color.textSecondary
    }

    ArtTile {
        id: art
        visible: !row.folder
        width: Metrics.folderArt; height: width
        x: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        radius: Metrics.radiusArtSmall
        source: !row.artwork ? "" : row.artwork.indexOf("://") >= 0 ? row.artwork : "file://" + row.artwork.split("/").map(encodeURIComponent).join("/")
        tint: row.index
    }

    Column {
        anchors.left: art.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: controls.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: row.name
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: row.folder
                ? I18n.trCount("{} playlist", "{} playlists", row.tracks, I18n.rev)
                : row.owner + " · " + I18n.trCount("{} track", "{} tracks", row.tracks, I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }

    Row {
        id: controls
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        IconButton {
            visible: row.custom && !row.folder
            size: Metrics.button24
            glyph: Icons.chevronUp
            tip: I18n.tr("Move up", I18n.rev)
            onClicked: row.manager.moveCustom(row.index, -1)
        }
        IconButton {
            visible: row.custom && !row.folder
            size: Metrics.button24
            glyph: Icons.chevronDown
            tip: I18n.tr("Move down", I18n.rev)
            onClicked: row.manager.moveCustom(row.index, 1)
        }
        IconButton {
            id: moreButton
            objectName: "playlistMore"
            size: Metrics.button24
            glyph: Icons.dotsVertical
            tip: I18n.tr("More actions", I18n.rev)
            onClicked: { const at = moreButton.mapToItem(row, moreButton.width - Metrics.menuWidth, moreButton.height + Metrics.gap4); row.menuAsked(at.x, at.y) }
        }
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onDoubleTapped: row.manager.open(row.index) }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: point => row.menuAsked(point.position.x, point.position.y) }
}
