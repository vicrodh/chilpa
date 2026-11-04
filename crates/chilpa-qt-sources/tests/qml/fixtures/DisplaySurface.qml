import QtQuick

// A plugin surface that offers the display contract, for the mount's tests.
Item {
    property var display: QtObject {
        property int version: 1
        property var host: null
        property bool useCustomPlayer: false
        property Component playerComponent: null
        property var menuEntries: []
        function command(action) {}
    }
}
