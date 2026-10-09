pragma Singleton
import QtQuick

// The waveform's numbers (borradores/waveform-seek-2026-10-08 §4, as the
// owner tightened it on the 9th, after a QuickLook player): a column every
// two pixels up to 512, rectangular bars that touch, never shorter than a
// pixel, over a rail along the middle.
QtObject {
    readonly property real pixelsPerColumn: 2
    readonly property int columnsMin: 48
    readonly property int columnsMax: 512
    // Half the bar's reach, as a share of the height: the bars never touch
    // the edges.
    readonly property real halfShare: 0.47
    readonly property real barLeast: 1
    readonly property real railOpacity: 0.46
    readonly property int railTall: 1
    readonly property int cursorWide: 1
    readonly property real cursorRest: 0.6
    readonly property int refreshMs: 100
}
