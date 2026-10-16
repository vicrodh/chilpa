import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// A button of the mini player's footer (02-mini §1.5, QBZ's TBtn): a disc
// that fills under the pointer, its glyph at idle strength until it is
// active or pointed. The glyph's color does not look at `able`, as QBZ's
// does not: only the whole button's opacity says it cannot be used.
Rectangle {
    id: button

    property string glyph
    property bool filled: false
    property int iconSize: Metrics.iconMd
    // The square's edge; the radius is half of it, so the fill is a disc.
    property int side: Metrics.button30
    property bool active: false
    // White glyphs over the dynamic background on a dark theme.
    property bool lightForeground: false
    // `enabled` is the item's own and would disarm the subtree.
    property bool able: true

    signal clicked()

    width: side
    height: side
    radius: side / 2 // derived: a disc
    antialiasing: true
    opacity: able ? 1 : Card.disabled
    color: area.containsMouse && able ? Theme.color.miniHover : "transparent"

    Text {
        text: button.glyph
        font.family: button.filled ? Icons.filled : Icons.outline
        font.pixelSize: button.iconSize
        width: button.iconSize
        height: button.iconSize
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        // Pixel-snapped, so a glyph of an odd size is not blurred.
        x: Math.round((button.width - width) / 2) // derived: centered
        y: Math.round((button.height - height) / 2) // derived: centered
        color: button.active ? Theme.color.accent : button.lightForeground ? Theme.color.miniGlyphOnBackdrop : Theme.color.textPrimary
        opacity: button.active || area.containsMouse ? 1 : Card.idleGlyph
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: button.able ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (button.able) button.clicked()
    }
}
