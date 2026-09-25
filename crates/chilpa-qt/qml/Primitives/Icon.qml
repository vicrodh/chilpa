import QtQuick
import "../Theme"

// A Tabler glyph drawn as text: colored like text and as cheap as a letter.
Text {
    property string glyph
    property bool filled: false
    property int size: Metrics.iconNav

    width: size
    height: size
    text: glyph
    font.family: filled ? Icons.filled : Icons.outline
    font.pixelSize: size
    color: Theme.color.textSecondary
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
}
