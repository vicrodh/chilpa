import QtQuick
import app.chilpa.controls

// Command searches and normal-mode result steps reach the existing flyout.
Connections {
    required property var host
    required property var search
    required property var field
    required property var flyout
    property bool active: true
    target: Hotkeys
    property Connections visibility: Connections {
        target: flyout
        function onClosed() { if (active) Hotkeys.searchActive = false }
        function onOpened() { if (active) Hotkeys.searchActive = search.text !== "" }
    }
    function onSearchCommand(query, source) {
        if (!active) return
        search.source = source
        search.text = query
        field.text = query
        flyout.ownInput = host.compact
        flyout.open()
        Hotkeys.searchActive = true
        host.forceActiveFocus()
    }
    function onSearchStepped(by) {
        if (active && search.text !== "") { flyout.open(); flyout.step(by) }
    }
    function onSearchConfirmed() { if (active && flyout.opened) flyout.chooseCurrent() }
}
