// hot-path: one per visible track, recycled while the list scrolls.
import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// What a track row starts with: the arrows of a list kept in its own
// order, and then its number, the mark of the one playing, or the check of
// a list being selected from.
Item {
    id: lead

    property int number: 0
    property bool playing: false
    property bool inViewport: true
    property bool selected: false
    property bool selectionMode: false
    property bool reorder: false
    // The arrow under the pointer: 1 up, 2 down, 0 neither.
    property int moveHovered: 0

    // The room the arrows take, before the number.
    property int gutter: 0
    property bool stacked: false

    Text {
        objectName: "trackNumber"
        x: lead.gutter
        width: lead.width - lead.gutter - Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        visible: !lead.selectionMode && !lead.playing
        text: lead.number > 0 ? lead.number : ""
        horizontalAlignment: Text.AlignRight
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    PlayingIndicator {
        x: lead.width - Metrics.gap8 - width
        anchors.verticalCenter: parent.verticalCenter
        visible: !lead.selectionMode && lead.playing
        running: lead.inViewport && Player.playing && !Player.paused; onToggled: Player.toggle()
    }

    Icon {
        x: lead.gutter + (lead.width - lead.gutter - width) / 2
        anchors.verticalCenter: parent.verticalCenter
        visible: lead.selectionMode
        glyph: lead.selected ? Icons.squareCheckFilled : Icons.square
        filled: lead.selected
        size: Metrics.iconMd
        color: lead.selected ? Theme.color.accent : Theme.color.textTertiary
    }

    // Only a list kept in its own order builds the arrows.
    Loader {
        active: lead.reorder && !lead.selectionMode
        height: parent.height
        sourceComponent: TrackRowMove { stacked: lead.stacked; hovered: lead.moveHovered }
    }
}
