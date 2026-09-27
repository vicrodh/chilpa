import QtQuick
import "../Theme"
import "../Primitives"

// An album in the folder content's list representation.
Item {
    id: row

    required property int index
    property int albumId
    property string title
    property string artist
    property url art

    signal open()

    height: Metrics.folderAlbumRow

    ListRowSurface { anchors.fill: parent; index: row.index; radius: Metrics.radiusControl; hovered: hover.hovered }
    ArtTile { id: cover; x: Metrics.gap6; anchors.verticalCenter: parent.verticalCenter; width: Metrics.folderArt; height: Metrics.folderArt; radius: Metrics.radiusArtSmall; source: row.art }
    Column {
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: chevron.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        Text { width: parent.width; text: row.title; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
        Text { width: parent.width; text: row.artist; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textSecondary }
    }
    Icon { id: chevron; anchors.right: parent.right; anchors.rightMargin: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter; glyph: Icons.chevronRight; size: Metrics.iconSm; color: Theme.color.textTertiary }
    HoverHandler { id: hover }
    TapHandler { onTapped: row.open() }
}
