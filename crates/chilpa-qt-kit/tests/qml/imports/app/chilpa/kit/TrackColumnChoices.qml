import QtQuick
QtObject {
    property string key: ""
    property int filterRev: 0
    property string columnResult: ""
    property bool columnBusy: false
    function requestColumn(field, search, cursor) {
        columnResult = JSON.stringify({field, search, cursor, items:[], next:"", more:false, error:""})
    }
    function closeColumn() { columnResult = ""; columnBusy = false }
}
