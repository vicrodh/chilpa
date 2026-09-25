pragma Singleton
import QtQuick

// Every measure the window uses, in logical pixels (design/01-tokens.md and
// design/02-layout.md). Nothing outside Theme/ writes a size by hand.
QtObject {
    // The reference window, and the smallest one the layout is drawn for.
    readonly property int windowWidth: 1280
    readonly property int windowHeight: 800
    readonly property int windowMinWidth: 960
    readonly property int windowMinHeight: 600
}
