pragma Singleton
import QtQuick

// The measures of what this module draws and the window's theme does not
// already name.
QtObject {
    // What a sheet has besides its columns: title, foot and the gaps.
    readonly property int sheetChrome: 150
    // A row of the sheet, and the least a key cap is wide in the editor.
    readonly property int keyRow: 36
    readonly property int keycapMin: 120
    // The countdown of the timer is read again this often.
    readonly property int second: 1000
    // What cannot be used right now.
    readonly property real idle: 0.3
}
