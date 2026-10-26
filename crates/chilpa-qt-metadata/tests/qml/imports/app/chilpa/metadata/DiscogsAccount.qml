pragma Singleton
import QtQuick
QtObject {
    property int revision: 0
    property bool busy: false
    property int reads: 0
    function refresh() { reads++ }
}
