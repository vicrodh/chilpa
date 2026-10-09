pragma Singleton
import QtQuick

// The waveform's numbers (borradores/waveform-seek-2026-10-08 §4): one
// column per 7.5 px between 48 and 240, bars a little over half their
// slot, rounded, in groups of twelve with a hairline of air between the
// groups, never shorter than 1.7 px, over a rail along the middle.
QtObject {
    readonly property real pixelsPerColumn: 7.5
    readonly property int columnsMin: 48
    readonly property int columnsMax: 240
    readonly property real barShare: 0.56
    readonly property real barMin: 1.8
    readonly property real barMax: 3.6
    readonly property int groupOf: 12
    readonly property real gapShare: 0.34
    readonly property real gapMin: 0.9
    readonly property real gapMax: 2.4
    // Half the bar's reach, as a share of the height: the bars never touch
    // the edges.
    readonly property real halfShare: 0.47
    readonly property real barLeast: 1.7
    readonly property real railOpacity: 0.46
    readonly property int railTall: 1
    readonly property int cursorWide: 1
    readonly property real cursorRest: 0.6
    readonly property int refreshMs: 100
}
