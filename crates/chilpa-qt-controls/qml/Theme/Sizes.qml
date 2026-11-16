pragma Singleton
import QtQuick

// The measures of what this module draws and the window's theme does not
// already name.
QtObject {
    // A row of the sheet, and the least a key cap is wide in the editor.
    readonly property int keyRow: 36
    readonly property int keycapMin: 120
    readonly property int shortcutCell: 280
    // The countdown of the timer is read again this often.
    readonly property int second: 1000
    // The window is still this long before its place is written down.
    readonly property int placeSettle: 400
    // The most the startup cover waits for a reading that never answers.
    readonly property int startupLimit: 30000
    // What cannot be used right now.
    readonly property real idle: 0.3
}
