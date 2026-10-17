pragma Singleton
import QtQuick
QtObject {
    property bool busy: false
    property bool pending: false
    property int refreshes: 0
    function refresh() { ++refreshes; busy = pending }
}
