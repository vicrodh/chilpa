// hot-path: one per visible track, recycled while the list scrolls.
import QtQuick
import "../Theme"
import "../Primitives"

// What a track row starts with, in the column of its number: the number,
// the mark of the one playing, the check of a list being selected from, or
// the arrows of a list kept in its own order.
Item {
    id: lead

    property int number: 0
    property bool playing: false
    property bool selected: false
    property bool selectionMode: false
    property bool reorder: false
    // The arrow under the pointer: 1 up, 2 down, 0 neither.
    property int moveHovered: 0

    readonly property bool plain: !selectionMode && !reorder

    Text {
        width: lead.width - Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        visible: lead.plain && !lead.playing
        text: lead.number > 0 ? lead.number : ""
        horizontalAlignment: Text.AlignRight
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    PlayingIndicator {
        x: lead.width - Metrics.gap8 - width
        anchors.verticalCenter: parent.verticalCenter
        visible: lead.plain && lead.playing
    }

    Icon {
        anchors.centerIn: parent
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
        sourceComponent: TrackRowMove { hovered: lead.moveHovered }
    }
}
