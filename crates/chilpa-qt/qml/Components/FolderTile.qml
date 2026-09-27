import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A navigable directory in the content grid: compact and visually distinct
// from an album card, because opening it changes place rather than record.
Item {
    id: tile

    required property string name
    required property string path
    required property int count
    required property url art

    signal open()

    implicitHeight: Metrics.folderTileHeight

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: hover.hovered ? Theme.color.stateHover : Theme.color.surfacePane
        Edge {}
    }

    Item {
        id: lead
        x: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.folderArt
        height: Metrics.folderArt

        ArtTile {
            anchors.fill: parent
            visible: tile.art.toString() !== ""
            radius: Metrics.radiusArtSmall
            source: tile.art
        }
        Icon {
            anchors.centerIn: parent
            visible: tile.art.toString() === ""
            glyph: Icons.folder
            size: Metrics.iconNav
            color: Theme.color.textSecondary
        }
    }

    Column {
        anchors.left: lead.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: chevron.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter

        Text { width: parent.width; text: tile.name; elide: Text.ElideMiddle; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
        Text { visible: tile.count > 0; text: I18n.trCount("{} track", "{} tracks", tile.count, I18n.rev); font.pointSize: Type.micro; color: Theme.color.textTertiary }
    }

    Icon {
        id: chevron
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronRight
        size: Metrics.iconSm
        color: Theme.color.textTertiary
    }

    HoverHandler { id: hover }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: tile.open() }
}
