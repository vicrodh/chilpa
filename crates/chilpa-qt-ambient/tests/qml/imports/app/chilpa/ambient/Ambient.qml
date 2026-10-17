pragma Singleton
import QtQuick

// The background's bridge as the tests see it.
QtObject {
    property int mode: 0
    property bool on: false
    property color primary: "#00dcc8"
    property color secondary: "#9632ff"
    property color accent: "#3fd9c8"
    property color glow: "#6464ff59"
    property color lifted: "#3fd9c8"
    property color brightest: "#3fd9c8"
    property string ringRampJson: "[]"
    property string tunnelPaletteJson: '["#ff6a6a","#ffcd5c","#68dcaa","#6eb0ff"]'
    property string wallpaperUrl: ""
    property string wallpaperAtmosphereUrl: ""
    property string wallpaperWindowsJson: "[]"
    property real wallpaperBlur: 0.75
    property string backgroundImage: ""
    property var followed: []
    property int reloads: 0
    property int refreshes: 0
    property int tracks: 0
    function reload() { reloads += 1 }
    function choose(index) { mode = index }
    property var demands: ({})
    function watch(consumer, art, active) {
        const next = Object.assign({}, demands)
        if (active) {
            if (next[consumer] !== art.toString()) followed.push(art.toString())
            next[consumer] = art.toString()
        } else { delete next[consumer] }
        demands = next
    }
    function refreshWallpaper() { refreshes += 1 }
    function trackWindowPosition() { tracks += 1 }
}
