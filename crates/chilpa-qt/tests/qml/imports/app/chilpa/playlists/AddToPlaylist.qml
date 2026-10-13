pragma Singleton
import QtQuick
QtObject {
    property string trackIds: "[]"
    function show(ids) { trackIds = ids }
}
