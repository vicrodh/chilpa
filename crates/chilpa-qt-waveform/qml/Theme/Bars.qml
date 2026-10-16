pragma Singleton
import QtQuick

// The waveform's numbers (borradores/waveform-seek-2026-10-08 §4, as the
// owner tightened it on the 9th, after QuickLook and Nulloy): a column per
// pixel up to the bins there are, each bar from its downward peak to its
// upward one, bars that touch, never shorter than a pixel, over a rail
// along the middle, shaded toward the edges.
QtObject {
    readonly property real pixelsPerColumn: 1
    readonly property int columnsMin: 48
    readonly property int columnsMax: 4096
    // The shade over the bars: the bar's own surface, fading to nothing
    // at the rail, so the tips sit back a little as Nulloy's do.
    readonly property real shadeOpacity: 0.45
    // Half the bar's reach, as a share of the height: the bars never touch
    // the edges.
    readonly property real halfShare: 0.47
    readonly property real barLeast: 1
    readonly property real railOpacity: 0.46
    readonly property int railTall: 1
    readonly property int cursorWide: 1
    readonly property real cursorRest: 0.6
    readonly property int refreshMs: 100
    // How long the width rests before the columns are asked for again: a
    // panel's slide is 160 ms of widths that change every frame.
    readonly property int settleMs: 120
}
