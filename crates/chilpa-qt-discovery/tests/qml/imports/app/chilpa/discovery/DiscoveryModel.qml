import QtQuick
QtObject {
    property string document: ""
    property bool loading: false
    readonly property int count: JSON.parse(document || "{}").total || 0
    property int revision: 0
    property var requests: []
    property var seeks: []
    property int cancellations: 0
    onDocumentChanged: revision++
    function request(kind, key, text, options, revision) { requests = requests.concat([{kind:kind,key:key,text:text,options:options,revision:revision}]) }
    function seek(first, last) { seeks = seeks.concat([{first:first,last:last}]).slice(-8) }
    function entry(index, rev) { return JSON.stringify((JSON.parse(document || "{}").items || [])[index] || {}) }
    function cancel() { cancellations++; document="{}" }
}
