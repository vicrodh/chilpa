pragma Singleton
import QtQuick
QtObject {
    property var preferences: ({})
    function kept(key) { return preferences[key] || "" }
    function keep(key, value) { preferences[key] = value }
    function panelWidthAt(width, windowWidth) { return width }
    // The lines alone; the margin to cross one back is the facade's, tested there.
    function sidebarModeFrom(mode, width, windowWidth) { return width < 28 ? "closed" : width < 150 ? "collapsed" : "open" }
    function sidebarWidthAt(width, windowWidth) { return Math.max(212, Math.min(width, windowWidth * 0.4)) }
    property var placed: []
    function placeSidebar(mode, width, windowWidth) { placed = [mode, width, windowWidth] }
    property bool quickViewClick: false
    property bool lyricsOpen: false
    property bool queueOpen: false
    property bool extraControls: false
    property string albumHeader: "off"
    property string density: "compact"
    property bool playlistCollage: false
    property bool folderColors: true
    property string sidebarMode: "expanded"
    property bool middleClickTabs: false
}
