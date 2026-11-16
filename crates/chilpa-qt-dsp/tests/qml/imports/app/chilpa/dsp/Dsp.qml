pragma Singleton
import QtQuick
QtObject {
    property string snapshot: "{}"
    property string presetName: ""
    property bool available: false
    property bool active: false
    property var actions: []
    property int viewers: 0
    property int lightRefreshes: 0
    property int fullRefreshes: 0
    property var statusReply: null
    property string document: ""
    function refresh() {
        if (viewers === 0) lightRefreshes++
        else {
            fullRefreshes++
            if (document) snapshot = document
        }
        if (statusReply) {
            available = statusReply.available
            active = statusReply.active
            presetName = statusReply.name
        }
    }
    function watchView(watching) {
        viewers = Math.max(0, viewers + (watching ? 1 : -1))
        if (viewers === 0) snapshot = ""
        if (watching) refresh()
    }
    function exportPresets(input) { return JSON.stringify({format: "chilpa-dsp", version: 2, presets: [JSON.parse(snapshot).preset]}) }
    function command(action, input) { actions = actions.concat([{ action: action, input: JSON.parse(input) }]) }
}
