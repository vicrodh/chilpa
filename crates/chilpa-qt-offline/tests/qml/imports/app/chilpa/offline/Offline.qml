pragma Singleton
import QtQuick

// The test double of the Offline bridge: it answers each action from what a
// test sets and writes down what it was asked.
QtObject {
    signal requested(string kind, string selected)
    property var calls: []
    property var info: ({count:0})
    property var status: ({running:false,total:0})
    property var removal: ({ticket:"fixture-removal",count:2})
    // The page: what a menu staged, its listing and how reading it goes.
    property var selection: ({kind:"downloads",selected:"[]",count:0})
    property var doc: null
    property int revision: 1
    property bool loading: false
    property string said: ""
    function call(action, kind, selected) {
        calls = calls.concat([[action,kind,selected]])
        if (action === "info") return JSON.stringify(info)
        if (action === "stage") selection = kind === "downloads" ? {kind:"downloads",selected:"[]",count:0} : {kind:kind,selected:selected,count:info.count}
        if (action === "stage" || action === "selection") return JSON.stringify(selection)
        if (action === "manager") {
            const answer = {revision:revision,loading:loading,said:said}
            if (doc && String(revision) !== kind) answer.doc = doc
            return JSON.stringify(answer)
        }
        if (action === "refresh" || action === "dismiss_removal") return "{}"
        if (action === "limit") return JSON.stringify({ok:true})
        if (action === "server_folder") return JSON.stringify({pending:true})
        if (action === "removal_info") return JSON.stringify(removal)
        if (action === "remove") status = {running:true,removing:true,total:2,done:0,failed:0}
        if (action === "start") status = {running:true,total:info.count,done:0,failed:0}
        if (action === "cancel") status = Object.assign({},status,{cancelled:true})
        return JSON.stringify(status)
    }
}
