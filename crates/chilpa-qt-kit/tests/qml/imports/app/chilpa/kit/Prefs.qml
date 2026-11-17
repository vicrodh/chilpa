pragma Singleton
import QtQuick

QtObject {
    property int rev: 0
    property string previewKey: ""
    property string previewValue: ""
    property bool animatedIndicator: false
    property bool trackComposers: false
    property bool ratings: true
    property string sidebar: ""
    property bool protectLibrary: false
    property bool protectedSidecars: false
    function preview(key, value) { previewKey = key; previewValue = value }
    function changed() { rev += 1 }
}
