pragma Singleton
import QtQuick
QtObject {
    signal added(string path)
    property bool busy: false
    property int count: 1
    property int rev: 0
    property bool scanning: false
    property string scanningPath: ""
    property real progress: 0
    property string status: ""
    property string phase: "idle"
    property bool canResume: false
    property int found: 0
    property int done: 0
    property int total: 0
    property int readCount: 0
    property int sameCount: 0
    property int missingCount: 0
    property int unreadableCount: 0
    property bool unreachable: false
    property string action: ""
    property string actionPath: ""
    property string chosenPolicy: ""
    function stopScan() { action = "stop" }
    function resumeScan() { action = "resume" }
    function dismissStatus() { action = "dismiss" }
    function rescanAll() { action = "all" }
    function rescan(path) { action = "check"; actionPath = path }
    function choosePolicy(path, policy) { action = "policy"; actionPath = path; chosenPolicy = policy }
    property var analyzeAsked: []
    function offersAnalysis() { return !busy }
    function analyzeAfterScan(path) { analyzeAsked = analyzeAsked.concat([path]) }
}
