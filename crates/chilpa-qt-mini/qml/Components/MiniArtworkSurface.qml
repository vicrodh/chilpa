import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Surface 2, the default (02-mini §1.3): a large cover over centered
// text. Display only, elided like QBZ's. An empty artist or album takes
// its line away, and the gap with it: there is no fallback string here.
Item {
    id: surface
    objectName: "miniMetadata"

    property bool backdropActive: false
    readonly property bool lightText: backdropActive && Theme.dark
    readonly property color headerStrong: lightText ? Theme.color.miniGlyphOnBackdrop : Theme.color.textPrimary
    readonly property color headerBody: lightText ? Theme.color.miniOnBackdrop : Theme.color.textSecondary
    readonly property int coverSize: Math.min(surface.width - Card.artworkPadSide * 2, Card.coverMax) // derived: the padding on both sides

    // Centered inside a box padded more above than below, so the stack
    // sits a little under the middle, as QBZ's layout lands it.
    Column {
        id: stack
        x: Card.artworkPadSide
        width: surface.width - Card.artworkPadSide * 2 // derived: the padding on both sides
        y: Card.artworkPadTop + Math.max(0, (surface.height - Card.artworkPadTop - Card.artworkPadBottom - stack.height) / 2) // derived: centered in the padded box
        spacing: Card.artworkApart

        Item {
            width: stack.width
            height: surface.coverSize
            MiniCover {
                anchors.horizontalCenter: parent.horizontalCenter
                width: surface.coverSize
                height: surface.coverSize
                radius: Card.coverRadiusArtwork
                source: Player.art
            }
        }

        Column {
            width: stack.width
            spacing: Card.metaApart

            Text {
                width: parent.width
                text: Player.path !== "" ? Player.title : I18n.tr("No track playing", I18n.rev)
                color: surface.headerStrong
                font.pointSize: Card.titleText
                font.weight: Type.semibold
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: Player.artist !== ""
                text: Player.artist
                color: surface.headerBody
                font.pointSize: Card.bodyText
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                visible: Player.album !== ""
                text: Player.album
                color: surface.headerBody
                font.pointSize: Card.bodyText
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
        }
    }
}
