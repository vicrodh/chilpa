import QtQuick
import QtQuick.Window

// I take the panel's unclaimed presses before passive handlers on the page.
// no cursor: I contain unused presses without changing the control under the pointer.
TapHandler {
    id: guard
    required property var surface
    readonly property var window: surface.parent ? surface.parent.Window.window : parent ? parent.Window.window : null
    parent: surface.contentItem
    target: null
    acceptedButtons: Qt.AllButtons
    gesturePolicy: TapHandler.WithinBounds

    function sync() { InputLayers.set(guard, !!surface && surface.visible) }
    onWindowChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: InputLayers.set(guard, false)
    // I cancel grabs from a press that started before the modal appeared.
    // The temporary binding restores the window's existing enabled binding.
    property bool cancelling: false
    property Binding cancellation: Binding {
        target: guard.window ? guard.window.contentItem : null
        property: "enabled"
        value: false
        when: guard.cancelling
        restoreMode: Binding.RestoreBindingOrValue
    }
    property Connections lifecycle: Connections {
        target: guard.surface
        function onAboutToShow() {
            if (guard.window && (guard.surface.modal || guard.surface.blocksBackground === true)) {
                const focused = guard.window.activeFocusItem
                // I release focus before enabled drives a focused control out of Tab order.
                if (focused) focused.focus = false
                guard.cancelling = true
                guard.cancelling = false
                if (focused) focused.forceActiveFocus()
            }
        }
        function onVisibleChanged() { guard.sync() }
    }
}
