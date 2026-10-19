pragma Singleton
import QtQuick

// The theme options' bridge as the tests see it: a document they set, and
// what it was asked.
QtObject {
    property var doc: ({
        theme: "custom", title: "Custom", filter: 0, light: ["light", "alucard"],
        auto: { sources: ["System Colors", "Wallpaper Sync", "Custom Image"], source: 0, image: "", desktop: "KDE Plasma", folded: false },
        custom: { state: { dark: true, colors: { "surface-main": "#000000", "accent": "#4285f4" } }, folded: false }
    })
    readonly property string state: JSON.stringify(doc)
    property var calls: []
    property int reloads: 0
    function record(what) { calls = calls.concat([what]) }
    function reload() { reloads += 1 }
    function setFilter(filter) { record("filter " + filter); doc = Object.assign({}, doc, { filter: filter }) }
    function setSource(index) { record("source " + index) }
    function setImage(path) { record("image " + path) }
    function regenerate() { record("regenerate") }
    function fold(which, folded) { record("fold " + which + " " + folded) }
    function useCurrentColors() { record("seed") }
    function setDark(dark) { record("dark " + dark) }
    function setColor(key, hex) { record("color " + key + " " + hex) }
    function flush() { record("flush") }
    function hsv(hue, saturation, value, alpha) { return Qt.hsva(hue, saturation, value, alpha) }
}
