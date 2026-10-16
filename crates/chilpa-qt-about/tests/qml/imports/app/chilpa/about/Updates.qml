pragma Singleton
import QtQuick
QtObject {
    property string snapshot: JSON.stringify({supported:false,phase:"idle",automatic:true,busy:false,progress:0})
    property var calls: []
    function refresh() {}
    function check(automatic) { calls = calls.concat(["check"]) }
    function download() { calls = calls.concat(["download"]) }
    function cancel() { calls = calls.concat(["cancel"]) }
    function setAutomatic(on) { calls = calls.concat(["automatic"]) }
    function restart() { calls = calls.concat(["restart"]); return false }
}
