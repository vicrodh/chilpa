import QtQuick
import app.chilpa.kit

// Cancel delegate creation before its owner replaces a creation context.
Loader {
    id: loader
    active: false
    readonly property bool retiring: lifetime.changing
    PageLifetime { id: lifetime; property bool changing: false }

    function replace(component, shown) {
        const next = shown ? component : null
        if (sourceComponent === next && active === shown) return
        lifetime.changing = true
        lifetime.cancel(loader)
        sourceComponent = next
        active = shown
        lifetime.changing = false
    }
}
