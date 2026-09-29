import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A folder inside the one that is open: a cover of what it holds, or a
// folder where it has none, its name and how many tracks are under it.
Item {
    id: card

    required property string name
    required property string path
    required property int count
    required property url art

    signal open()
    signal context(real left, real top)

    implicitHeight: width + Metrics.folderCardText

    Rectangle {
        anchors.fill: parent
        radius: Metrics.radiusCard
        color: Theme.color.surfacePane
        Edge { border.color: hover.hovered ? Theme.color.accent : Theme.color.strokeHairline }
    }

    Item {
        id: cover
        x: Metrics.gap8
        y: Metrics.gap8
        width: parent.width - Metrics.gap16 // derived: padding on both sides
        height: width

        ArtTile {
            anchors.fill: parent
            visible: card.art.toString() !== ""
            radius: Metrics.radiusArtSmall
            source: card.art
        }
        Icon {
            anchors.centerIn: parent
            visible: card.art.toString() === ""
            glyph: Icons.folder
            size: Metrics.folderGlyph
            color: Theme.color.textSecondary
        }
    }

    Column {
        anchors.top: cover.bottom
        anchors.topMargin: Metrics.gap4
        x: Metrics.gap8
        width: cover.width

        Text { width: parent.width; text: card.name; elide: Text.ElideMiddle; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
        Text { text: I18n.trCount("{} track", "{} tracks", card.count, I18n.rev); font.pointSize: Type.label; color: Theme.color.textSecondary }
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: card.open() }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: (point) => card.context(point.position.x, point.position.y) }
}
