// hot-path: one per visible track, recycled while the list scrolls.
import QtQuick
import app.chilpa
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// I place order arrows before the track number, playing mark or selection check.
Item {
    id: lead

    property int number: 0
    property bool showNumber: true
    property var columns: null
    readonly property real markWidth: Prefs.animatedIndicator && Player.playing && !Player.paused ? Metrics.eqWide : Metrics.iconMd
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
        visible: lead.showNumber && !lead.selectionMode && !lead.playing
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
        running: lead.inViewport && (!lead.columns || lead.columns.inView(lead.x + lead.width - Metrics.gap8 - lead.markWidth, lead.markWidth)) && Player.playing && !Player.paused; onToggled: Player.toggle()
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
