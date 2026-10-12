import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The scene keeps a consistent card footprint and row rhythm.
Rectangle {
    id: card
    objectName: "sceneArtistCard"
    property var artist: ({})
    property bool selected: false
    signal activated()
    width: Metrics.sceneCardWidth; height: Metrics.sceneCardHeight
    radius: Metrics.radiusCard
    color: selected ? Theme.color.rowSelected : hover.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
    border.width: selected ? 1 : 0
    border.color: Theme.color.accent
    readonly property var firstSource: (artist.sources || [])[0] || ({})

    ArtTile {
        id: portrait
        x: Metrics.gap8; y: Metrics.gap8
        width: parent.width - Metrics.gap16; height: width
        radius: width / Metrics.gap2
        source: card.firstSource.plugin && card.firstSource.image
            ? Sources.art(card.firstSource.plugin, card.firstSource.image, Library.remoteArtRev) : ""
    }
    Icon { anchors.centerIn: portrait; visible: !portrait.ready; glyph: SourceGlyphs.musicArtist; iconFamily: SourceGlyphs.font; size: Metrics.iconPlay; color: Theme.color.textTertiary }
    Text {
        id: nameText
        objectName: "sceneArtistTitle"
        x: Metrics.gap8; y: portrait.y + portrait.height + Metrics.gap8
        width: parent.width - Metrics.gap16; text: artist.title || ""; textFormat: Text.PlainText
        elide: Text.ElideRight; horizontalAlignment: Text.AlignHCenter
        font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: card.activated() }
}
