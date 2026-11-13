import QtQuick
QtObject {
    property string document: "{}"
    property bool loading: false
    property var requests: []
    property int cancellations: 0
    function request(kind, key, text, page, revision) { requests = requests.concat([{kind:kind,key:key,text:text,page:page,revision:revision}]) }
    function cancel() { cancellations++ }
}
