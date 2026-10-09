import QtQuick
import app.chilpa.lyrics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One line of lyrics as the panel shows it: its words, word by word while
// it is sung, and under them its translation when one is shown. The lines
// around the sung one fade the further they are from it, and each fades in
// as its turn comes and out once it went by. A click on a line that has a
// time goes to that time.
Column {
    id: row

    // The line as the lyrics document has it, or only its text.
    property var line: ({})
    // The lyrics follow the music; then its place among the lines and the
    // place of the line being sung, or -1 while none is.
    property bool synced: false
    property int position: -1
    property int current: -1
    readonly property bool sung: synced && position >= 0 && position === current
    readonly property bool past: synced && current >= 0 && position < current
    // The moment of the track and where the line ends, for the sung line.
    property real nowMs: 0
    property real endMs: 0
    property string translation
    // How it is drawn: its size, the color of what is sung, how much the
    // other lines fade ("off", "soft" or "strong"), capitals, and whether
    // the sung line fills word by word.
    property real size: Type.lyric
    property color sungColor: Theme.color.textPrimary
    // The lines around the sung one, and the part of it not yet sung; the
    // ones already sung. The theme's by default, fixed whites over a field.
    property color dimColor: Theme.color.textSecondary
    property color pastColor: Theme.color.textTertiary
    property string dimming: "strong"
    property bool uppercase: false
    property bool wordByWord: true
    // Centered lines and a raised shadow under them, for a line alone over
    // a cover; off, the panel's left alignment and no shadow.
    property bool centered: false
    property color shadowColor: "transparent"
    // How lit it is, from 0 to 1.
    readonly property real lit: LyricsView.dim(dimming, synced, position, current)

    signal sought(real ms)

    readonly property bool whole: typeof line === "string"
    readonly property string text: whole ? line : (line.text || "")
    readonly property bool timed: !whole && line.start_ms !== undefined

    spacing: Metrics.gap2

    Loader {
        objectName: "words"
        width: row.width
        opacity: row.lit
        sourceComponent: row.sung && row.wordByWord && !row.whole ? words : plain

        Behavior on opacity { NumberAnimation { duration: Motion.lyricDim; easing.type: Easing.OutQuad } }

        Component {
            id: plain
            Text {
                text: row.text
                wrapMode: Text.Wrap
                horizontalAlignment: row.centered ? Text.AlignHCenter : Text.AlignLeft
                font.pointSize: row.size
                font.weight: Type.bold
                font.capitalization: row.uppercase ? Font.AllUppercase : Font.MixedCase
                color: row.sung ? row.sungColor : row.past ? row.pastColor : row.dimColor
                style: row.shadowColor.a > 0 ? Text.Raised : Text.Normal
                styleColor: row.shadowColor
            }
        }
        Component {
            id: words
            LyricWords {
                objectName: "sungLine"
                line: row.line
                endMs: row.endMs
                nowMs: row.nowMs
                font.pointSize: row.size
                font.capitalization: row.uppercase ? Font.AllUppercase : Font.MixedCase
                sungColor: row.sungColor
                restColor: row.dimColor
                centered: row.centered
                shadowColor: row.shadowColor
            }
        }
    }

    // The translation follows its line, a little fainter, and is never set
    // in capitals: it was written by somebody else.
    Text {
        objectName: "translation"
        visible: row.translation !== ""
        width: row.width
        opacity: row.lit * (row.past ? 1 : 0.7)
        text: row.translation
        wrapMode: Text.Wrap
        horizontalAlignment: row.centered ? Text.AlignHCenter : Text.AlignLeft
        font.pointSize: row.size * Type.lyricTranslation // derived: a share of the line's size
        color: row.past ? row.pastColor : row.dimColor

        Behavior on opacity { NumberAnimation { duration: Motion.lyricDim; easing.type: Easing.OutQuad } }
    }

    HoverHandler { enabled: row.timed; cursorShape: Qt.PointingHandCursor }
    TapHandler { enabled: row.timed; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: row.sought(row.line.start_ms) }
}
