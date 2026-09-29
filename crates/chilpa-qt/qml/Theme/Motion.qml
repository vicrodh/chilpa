pragma Singleton
import QtQuick

// Durations (design/01-tokens.md 5). With reduced motion every one is zero,
// so nothing has to ask for itself whether to animate.
QtObject {
    property bool reduced: false

    readonly property int fast: reduced ? 0 : 120
    readonly property int base: reduced ? 0 : 150
    readonly property int panel: reduced ? 0 : 200
    readonly property int ambient: reduced ? 0 : 600
    readonly property int toast: 2200
    // How often a view reads again what moves on another thread: a setup
    // being waited for, a sync. Not motion: never reduced.
    readonly property int poll: 300
    readonly property int refresh: 1000
    // How long after the last key what was typed is asked for.
    readonly property int searchSettle: 250
    readonly property int treeSettle: 180
}
