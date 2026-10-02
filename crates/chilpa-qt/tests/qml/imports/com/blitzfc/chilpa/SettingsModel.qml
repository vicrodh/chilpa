pragma Singleton
import QtQuick
QtObject {
    property var preferences: ({})
    function kept(key) { return preferences[key] || "" }
    function keep(key, value) { preferences[key] = value }
    property bool quickViewClick: false
    property string albumHeader: "off"
    property string density: "compact"
    property bool folderColors: true
    property string sidebarMode: "expanded"
    property bool middleClickTabs: false
}
