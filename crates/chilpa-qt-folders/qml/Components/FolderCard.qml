import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A folder inside the one that is open, and said to be one: a folder's mark
// over a cover of what it holds, its name, and how many folders and tracks
// are in it. A folder that is a record is not drawn with this: it is a record.
Item {
    id: card

    required property string name
    required property string path
    required property int count
    required property int folders
    required property double album
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
        // Over a cover, what says this is a folder and not the record.
        Rectangle {
            visible: card.art.toString() !== ""
            x: Metrics.gap6
            y: Metrics.gap6
            width: Metrics.button26
            height: Metrics.button26
            radius: Metrics.radiusItem
            color: Theme.color.overlayButton

            Icon {
                anchors.centerIn: parent
                glyph: Icons.folder
                size: Metrics.iconSm
                color: Theme.color.overlayInk
            }
        }
    }

    Column {
        anchors.top: cover.bottom
        anchors.topMargin: Metrics.gap4
        x: Metrics.gap8
        width: cover.width

        Text { width: parent.width; text: card.name; elide: Text.ElideMiddle; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
        Text {
            width: parent.width
            text: (card.folders > 0 ? I18n.trCount("{} folder", "{} folders", card.folders, I18n.rev) + " · " : "")
                  + I18n.trCount("{} track", "{} tracks", card.count, I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.label
            color: Theme.color.textSecondary
        }
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: card.open() }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: (point) => card.context(point.position.x, point.position.y) }
}
