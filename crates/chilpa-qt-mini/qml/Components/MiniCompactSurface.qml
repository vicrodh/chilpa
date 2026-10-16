import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Surface 1 (02-mini §1.3): a strip of cover and text in the 380 by 178
// window. Display only: no controls, no marquee, long text is cut with an
// ellipsis. The artist line is always there, with a dash when there is
// none, where the artwork surface drops the line; both are QBZ's.
Item {
    id: surface
    objectName: "miniMetadata"

    property bool backdropActive: false
    readonly property bool lightText: backdropActive && Theme.dark
    readonly property color headerStrong: lightText ? Theme.color.miniGlyphOnBackdrop : Theme.color.textPrimary
    readonly property color headerBody: lightText ? Theme.color.miniOnBackdrop : Theme.color.textSecondary
    // The cover is square: the surface less its padding above and below.
    readonly property int art: Math.max(Card.coverMin, surface.height - Card.compactPad * 2) // derived: the padding on both sides

    MiniCover {
        id: cover
        x: Card.compactPad
        anchors.verticalCenter: parent.verticalCenter
        width: surface.art
        height: surface.art
        radius: Card.coverRadiusCompact
        source: Player.art
    }

    Column {
        id: meta
        anchors.left: cover.right
        anchors.leftMargin: Card.compactPad
        anchors.right: parent.right
        anchors.rightMargin: Card.compactPad
        anchors.verticalCenter: parent.verticalCenter
        spacing: Card.metaApart

        Text {
            width: meta.width
            text: Player.path !== "" ? Player.title : I18n.tr("No track playing", I18n.rev)
            color: surface.headerStrong
            font.pointSize: Card.titleText
            font.weight: Type.semibold
            elide: Text.ElideRight
        }
        Text {
            width: meta.width
            text: Player.artist !== "" ? Player.artist : "—"
            color: surface.headerBody
            font.pointSize: Card.metaText
            elide: Text.ElideRight
        }
    }
}
