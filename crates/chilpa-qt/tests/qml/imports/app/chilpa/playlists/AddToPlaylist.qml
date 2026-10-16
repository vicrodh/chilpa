pragma Singleton
import QtQuick
QtObject {
    property string trackIds: "[]"
    function finish() {}
    function show(ids) { trackIds = ids }
}
