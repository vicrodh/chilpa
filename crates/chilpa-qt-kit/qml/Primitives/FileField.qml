import QtQuick
import QtQuick.Dialogs
import "../Theme"

// A file chosen from a dialog and, once one is chosen, a way back to none
// (design/03 SettingsRow, file). The path comes out plain, never as a URL.
Row {
    id: field

    property string action
    property string value
    property string title
    property string clearTip
    property var filters: []

    signal picked(string path)
    signal cleared()

    spacing: Metrics.gap8

    PillButton {
        anchors.verticalCenter: parent.verticalCenter
        text: field.action
        onClicked: picker.open()
    }
    IconButton {
        anchors.verticalCenter: parent.verticalCenter
        visible: field.value !== ""
        tip: field.clearTip
        glyph: Icons.x
        size: Metrics.button26
        onClicked: field.cleared()
    }
    FileDialog {
        id: picker
        title: field.title
        nameFilters: field.filters
        onAccepted: field.picked(decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, "")))
    }
}
