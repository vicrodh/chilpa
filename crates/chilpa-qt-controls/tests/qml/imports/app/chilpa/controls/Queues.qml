pragma Singleton
import QtQuick
QtObject {
    property var merged: []
    property string error: ""
    function merge(ids, name, keep) { merged = [JSON.parse(ids), name, keep]; return error }
}
