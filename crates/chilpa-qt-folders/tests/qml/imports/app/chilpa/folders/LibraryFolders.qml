pragma Singleton
import QtQuick
QtObject {
    signal added()
    property bool busy: false
    property int analyzeAsked: 0
    function offersAnalysis() { return !busy }
    function analyzeAfterScan() { analyzeAsked++ }
}
