import QtQuick
ListModel {
    property string text: ""
    property string source: "all"
    function refresh() {}
    property string scope: "all"
    property string qualityFilter: ""
    property bool hideExplicit: false
    function act(index, action) {}
}
