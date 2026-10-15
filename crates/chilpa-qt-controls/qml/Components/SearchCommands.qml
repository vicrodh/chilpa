import QtQuick
import app.chilpa.controls

// Command searches and normal-mode result steps reach the existing flyout.
Connections {
    id: route
    required property var host
    required property var search
    required property var field
    required property var flyout
    property bool active: true
    property var origin: null
    property bool commandFocus: false
    property Item navigation: Item {
        parent: route.active ? route.flyout.contentItem : null
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Down) route.flyout.step(1)
            else if (event.key === Qt.Key_Up) route.flyout.step(-1)
            else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) route.flyout.chooseCurrent()
            else if (event.key === Qt.Key_Escape) route.flyout.close()
            else return
            event.accepted = true
        }
    }
    target: Hotkeys
    property Connections visibility: Connections {
        property var router: route
        target: flyout
        function onClosed() { this.router.searchClosed() }
        function onOpened() { this.router.searchOpened() }
    }
    function searchClosed() {
        if (!active) return
        Hotkeys.searchActive = false
        if (commandFocus && route.origin && route.origin.visible) route.origin.forceActiveFocus()
        commandFocus = false
        route.origin = null
    }
    function searchOpened() { if (active) Hotkeys.searchActive = search.text !== "" }
    // Connections signal handlers can run without the component's id scope.
    // Bound methods retain it for focus ownership and result navigation.
    function onSearchCommand(query, source) { this.openCommand(query, source) }
    function openCommand(query, source) {
        if (!active) return
        if (!route.commandFocus) route.origin = route.host.Window.window.activeFocusItem
        route.commandFocus = true
        search.source = source
        search.text = query
        field.text = query
        flyout.ownInput = host.compact
        flyout.open()
        Hotkeys.searchActive = true
        navigation.forceActiveFocus()
    }
    function onSearchStepped(by) { this.stepSearch(by) }
    function stepSearch(by) {
        if (active && search.text !== "") { flyout.open(); flyout.step(by) }
    }
    function onSearchConfirmed() { this.confirmSearch() }
    function confirmSearch() { if (active && flyout.opened) flyout.chooseCurrent() }
}
