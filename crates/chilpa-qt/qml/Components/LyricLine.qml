import QtQuick
import "../Theme"

// One line of lyrics as the panel shows it: its words, word by word while
// it is sung, and under them its translation when one is shown. A click on
// a line that has a time goes to that time.
Column {
    id: row

    // The line as the lyrics document has it, or only its text.
    property var line: ({})
    // The lyrics follow the music, and this is the line being sung.
    property bool synced: false
    property bool sung: false
    // The moment of the track and where the line ends, for the sung line.
    property real nowMs: 0
    property real endMs: 0
    property string translation
    // How it is drawn: its size, the color of what is sung and of the rest,
    // capitals, and whether the sung line fills word by word.
    property real size: Type.lyric
    property color sungColor: Theme.color.textPrimary
    property color restColor: Theme.color.textTertiary
    property bool uppercase: false
    property bool wordByWord: true

    signal sought(real ms)

    readonly property bool whole: typeof line === "string"
    readonly property string text: whole ? line : (line.text || "")
    readonly property bool timed: !whole && line.start_ms !== undefined

    spacing: Metrics.gap2

    Loader {
        width: row.width
        sourceComponent: row.sung && row.wordByWord && !row.whole ? words : plain

        Component {
            id: plain
            Text {
                text: row.text
                wrapMode: Text.Wrap
                font.pointSize: row.size
                font.weight: Type.bold
                font.capitalization: row.uppercase ? Font.AllUppercase : Font.MixedCase
                color: !row.synced ? Theme.color.textPrimary : row.sung ? row.sungColor : row.restColor
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
                restColor: row.restColor
            }
        }
    }

    // The translation follows its line and is never set in capitals: it
    // was written by somebody else.
    Text {
        objectName: "translation"
        visible: row.translation !== ""
        width: row.width
        text: row.translation
        wrapMode: Text.Wrap
        font.pointSize: row.size * Type.lyricTranslation // derived: a share of the line's size
        color: row.sung || !row.synced ? Theme.color.textSecondary : Theme.color.textTertiary
    }

    HoverHandler { enabled: row.timed; cursorShape: Qt.PointingHandCursor }
    TapHandler { enabled: row.timed; gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: row.sought(row.line.start_ms) }
}
