import QtQuick
import "../Theme"

// A Tabler glyph drawn as text: colored like text and as cheap as a letter.
Text {
    property string glyph
    property bool filled: false
    property string iconFamily
    property int size: Metrics.iconNav

    width: size
    height: size
    text: ""
    readonly property string desiredFamily: iconFamily !== "" ? iconFamily : filled ? Icons.filled : Icons.outline
    function updateGlyph() {
        if (desiredFamily === "" || text === glyph && font.family === desiredFamily) return
        // I change the family without shaping either glyph in the other font.
        text = ""
        font.family = desiredFamily
        text = glyph
    }
    onGlyphChanged: Qt.callLater(updateGlyph)
    onDesiredFamilyChanged: Qt.callLater(updateGlyph)
    Component.onCompleted: updateGlyph()
    font.pixelSize: size
    color: Theme.color.textSecondary
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
}
