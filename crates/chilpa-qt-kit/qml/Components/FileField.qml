import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// A file chosen with the system's dialog and, once one is chosen, a way back
// to none (design/03 SettingsRow, file). The path comes out plain.
Row {
    id: field

    property string action
    property string value
    property string title
    property string clearTip
    // In Qt's form, "Images (*.png *.jpg)"; one that takes every file, "(*)",
    // is no filter at all.
    property var filters: []
    readonly property string asker: "fileField " + String(field)

    signal picked(string path)
    signal cleared()

    function dialogFilters() {
        return JSON.stringify(filters.map(filter => {
            const parts = /^(.*?)\s*\(([^)]*)\)\s*$/.exec(filter)
            const extensions = parts ? parts[2].split(/\s+/).map(e => e.replace(/^\*\.?/, "")).filter(e => e !== "") : []
            return {name: parts ? parts[1] : filter, extensions: extensions}
        }).filter(filter => filter.extensions.length > 0))
    }

    spacing: Metrics.gap8

    PillButton {
        anchors.verticalCenter: parent.verticalCenter
        text: field.action
        onClicked: SystemDialogs.pickFile(field.asker, field.title, field.dialogFilters())
    }
    IconButton {
        anchors.verticalCenter: parent.verticalCenter
        visible: field.value !== ""
        tip: field.clearTip
        glyph: Icons.x
        size: Metrics.button26
        onClicked: field.cleared()
    }
    Connections {
        target: SystemDialogs
        function onChosen(asker, path) { if (asker === field.asker) field.picked(path) }
    }
}
