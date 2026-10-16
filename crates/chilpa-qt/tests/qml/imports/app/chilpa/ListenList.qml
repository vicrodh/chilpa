pragma Singleton
import QtQuick
QtObject {
    property string snapshot: JSON.stringify({ selected: "1", active: "1", name: "Jazz", tabs: [{id:"1",name:"Jazz",count:3,playing:true},{id:"2",name:"Other",count:2,playing:false}], undo:false })
    property int revision: 0
    property string lastAction: ""
    property string lastId: ""
    property string renamedId: ""
    property string renamedText: ""
    property string footing: JSON.stringify({ summary: "3 songs 1 hr 2 min", left_ms: 600000 })
    function footer() { return footing }
    function refresh() {}
    function command(action, id, text) {
        lastAction = action; lastId = id
        if (action === "rename") { renamedId = id; renamedText = text }
        if (action === "select") {
            let doc = JSON.parse(snapshot)
            doc.selected = id
            doc.name = doc.tabs.find(tab => tab.id === id).name
            snapshot = JSON.stringify(doc)
            revision++
        }
    }
}
