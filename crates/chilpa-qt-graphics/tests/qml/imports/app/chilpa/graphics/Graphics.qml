pragma Singleton
import QtQuick

// The graphics bridge as the tests see it: it keeps what it was told.
QtObject {
    property string api: "unknown"
    property bool gpuTier: true
    property real scale: 1
    property int livenessWindow: 300
    property int apiReports: 0
    property int livenessReports: 0
    property int liveness: -1
    property int shown: 0
    property string notices: ""
    function reportRendererApi(name) { api = name; apiReports += 1 }
    function reportFrameLiveness(frames) { liveness = frames; livenessReports += 1 }
    function startupShown() { shown += 1 }
    function takeNotices() { const said = notices; notices = ""; return said }
}
