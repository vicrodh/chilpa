pragma Singleton
import QtQuick
import app.chilpa.kit

// The test double of the Offline bridge: it answers each action from what a
// test sets and writes down what it was asked.
QtObject {
    signal requested(string kind, string selected)
    property var calls: []
    property var info: ({count:0})
    property var status: ({running:false,total:0})
    property var removal: ({ticket:"fixture-removal",count:2})
    // The page: its listing and how reading it goes.
    property var doc: null
    property int revision: 1
    property bool loading: false
    property string said: ""
    // The rows: each track's {state, fraction, error} by id, each
    // selection's progress by kind and ids, and what a menu offers.
    property int rev: 0
    property bool active: false
    property var marks: ({})
    property var jobs: ({})
    property string menuCase: "none"
    function call(action, kind, selected) {
        calls = calls.concat([[action,kind,selected]])
        if (action === "info") return JSON.stringify(info)
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
        ++rev
        return JSON.stringify(status)
    }
    function mark(id) { return marks[id] || {} }
    function state(id, unused) { return mark(id).state || "" }
    function fraction(id, unused) { return mark(id).fraction === undefined ? -1 : mark(id).fraction }
    function error(id, unused) { return mark(id).error || "" }
    function tip(id, unused) {
        const words = {ready: "Available offline", queued: "Waiting in the offline queue", downloading: "Downloading for offline listening",
                       verifying: "Verifying downloaded audio…", removing: "Removing offline copy", failed: "Offline download failed"}
        const state = mark(id).state || ""
        return state === "failed" && mark(id).error ? mark(id).error : state ? I18n.tr(words[state], I18n.rev) : ""
    }
    function job(kind, selected, unused) { const found = jobs[kind + selected]; return found === undefined ? -1 : found }
    function menu(kind, selected) {
        calls = calls.concat([["menu",kind,selected]])
        if (!(info.count > 0)) return "[]"
        const remove = count => ({text:I18n.trCount("Remove offline copy", "Remove offline copies", count, I18n.rev), act:"offline:remove_now"})
        const cases = {
            none: [{text:I18n.tr("Make available offline", I18n.rev), act:"offline:start"}],
            kept: [{text:I18n.tr("Re-download", I18n.rev), act:"offline:redownload"}, remove(1)],
            failed: [{text:I18n.tr("Retry download", I18n.rev), act:"offline:start"}, remove(12)],
            busy: [{text:I18n.tr("Cancel download", I18n.rev), act:"offline:cancel_selection"}]
        }
        return JSON.stringify(cases[menuCase])
    }
    function refresh(reread) { calls = calls.concat([["refresh",reread]]); ++rev }
}
