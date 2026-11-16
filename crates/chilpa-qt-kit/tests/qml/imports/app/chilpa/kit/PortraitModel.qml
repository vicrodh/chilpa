import QtQuick
QtObject {
    property string name: ""
    property string mbid: ""
    property bool active: false
    property string source: ""
    property string credit: ""
    property string page: ""
    property int cancellations: 0
    onActiveChanged: if (!active) { source = ""; credit = ""; page = "" }
    onNameChanged: { source = ""; credit = ""; page = "" }
    function cancel() { cancellations++; source = ""; credit = ""; page = "" }
}
