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
    // A portrait already at hand, as the search page has; otherwise the
    // first source's.
    property url art
    property bool selected: false
    signal activated()
    signal sourceActivated(var source)
    signal menuAsked(real x, real y)
    width: Metrics.sceneCardWidth; height: Metrics.sceneCardHeight
    radius: Metrics.radiusCard
    color: selected ? Theme.color.rowSelected : hover.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
    border.width: selected ? 1 : 0
    border.color: Theme.color.accent
    readonly property var firstSource: (artist.sources || []).find(source => source.image) || ({})

    ArtTile {
        id: portrait
        x: Metrics.gap8; y: Metrics.gap8
        width: parent.width - Metrics.gap16; height: width
        radius: width / Metrics.gap2
        source: card.art.toString() !== "" ? card.art : card.firstSource.local ? card.firstSource.image || "" : card.firstSource.plugin && card.firstSource.image
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
    TapHandler { onTapped: point => { if (point.position.y < sourceButtons.y || !sourceButtons.visible) card.activated() } }
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: point => card.menuAsked(point.position.x, point.position.y) }
    Row {
        id: sourceButtons
        visible: (card.artist.sources || []).length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.gap4
        spacing: Metrics.gap6
        Repeater {
            model: card.artist.sources || []
            IconButton {
                required property var modelData
                objectName: "sceneArtistSource_" + (modelData.local ? "library" : modelData.plugin)
                size: Metrics.button24; round: true
                tip: modelData.name || modelData.plugin || "Chilpa"
                onClicked: card.sourceActivated(modelData)
                SourceIcon { anchors.centerIn: parent; width: Metrics.iconSm; height: width; sourceInfo: ({icon:modelData.icon || "",local:modelData.local === true,chilpa:modelData.chilpa === true}) }
            }
        }
    }
}
