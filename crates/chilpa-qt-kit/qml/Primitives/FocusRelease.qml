import QtQuick
import QtQuick.Window

// A press outside the text field that has the keyboard takes the keyboard
// from it, as QBZ's InputFocusDismiss does: the field commits what it holds
// and the window's keys answer again. It only watches, so the press still
// reaches what is under the pointer. A change of `key`, the view, does the
// same. The field lets go of its focus rather than a root taking it: the
// window's root is a focus scope and would hand it straight back.
Item {
    id: watch
    property var key
    anchors.fill: parent
    z: 100000
    function typing() {
        const window = watch.Window.window
        const item = window ? window.activeFocusItem : null
        return item instanceof TextInput || item instanceof TextEdit ? item : null
    }
    function release() {
        const item = watch.typing()
        if (item)
            item.focus = false
    }
    onKeyChanged: release()
    // A field that stops being seen while it has the keyboard lets go of it
    // for good: the root is a focus scope, and the next time it takes the
    // keyboard it would hand it straight back to the hidden field.
    property Item held: null
    Connections {
        target: watch.Window.window
        function onActiveFocusItemChanged() {
            const item = watch.typing()
            if (item)
                watch.held = item
        }
    }
    Connections {
        target: watch.held
        function onVisibleChanged() {
            if (watch.held && !watch.held.visible) {
                watch.held.focus = false
                watch.held = null
            }
        }
    }
    PointHandler {
        target: null
        acceptedButtons: Qt.LeftButton
        onActiveChanged: {
            const item = active ? watch.typing() : null
            if (item && !item.contains(item.mapFromItem(watch, point.position.x, point.position.y)))
                item.focus = false
        }
    }
}
