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
    function follow(art) { followed.push(art.toString()) }
    function refreshWallpaper() { refreshes += 1 }
    function trackWindowPosition() { tracks += 1 }
}
