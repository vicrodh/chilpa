pragma Singleton
import QtQuick
QtObject {
    signal added(string path)
    property bool busy: false
    property var analyzeAsked: []
    function offersAnalysis() { return !busy }
    function analyzeAfterScan(path) { analyzeAsked = analyzeAsked.concat([path]) }
}
