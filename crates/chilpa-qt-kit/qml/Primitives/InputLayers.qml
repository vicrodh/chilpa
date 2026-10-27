pragma Singleton
import QtQuick

// I retain only visible popups, including their enter and exit transitions.
QtObject {
    property var layers: []
    function set(layer, shown) {
        const next = layers.filter(entry => entry !== layer)
        if (shown) next.push(layer)
        layers = next
    }
    function blocks(window) {
        return layers.some(layer => layer.window === window
            && (layer.surface.modal || layer.surface.blocksBackground === true))
    }
    function blocksShortcuts(window) {
        return blocks(window) || layers.some(layer => layer.window === window && layer.surface.activeFocus)
    }
}
