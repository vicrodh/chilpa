pragma Singleton
import QtQuick

QtObject {
    property int rev: 0
    property string previewKey: ""
    property string previewValue: ""
    function preview(key, value) { previewKey = key; previewValue = value }
    function changed() { rev += 1 }
}
