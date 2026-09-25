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
}
