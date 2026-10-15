import QtQuick
import QtQuick.Window
import app.chilpa

Window {
    id: window
    visible: true
    width: 640
    height: 480
    property string pluginId: "PLUGIN_ID"
    property string pluginVersion: "PLUGIN_VERSION"
    property int step: 0
    SettingsPage { id: page; section: "plugins" }
    function fail(message) { console.error("SMOKE FAIL packaged process plugin: " + message); Qt.exit(1) }
    Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            const state = JSON.parse(page.pluginsJson())
            if (state.busy || state.working)
                return
            const installed = (state.installed || []).find(row => row.id === window.pluginId)
            if (!installed)
                return
            if (!installed.native || !installed.enabled || installed.state !== "fine" || installed.problem) {
                window.fail("the host could not load the packaged worker")
                return
            }
            if (installed.version !== window.pluginVersion) {
                window.fail("the host loaded another version")
                return
            }
            if (window.step === 0) {
                window.step = 1
                page.pluginCommand("settings", JSON.stringify({id: window.pluginId}))
            } else if (state.settings_id === window.pluginId && !state.settings_loading_id) {
                if (state.error || !(state.form?.form?.fields || []).length) {
                    window.fail("the packaged worker did not provide its settings form")
                    return
                }
                console.log("SMOKE packaged process plugin completed: exact version loaded, host permissions, settings RPC")
                Qt.quit()
            }
        }
    }
    Timer { interval: 30000; running: true; onTriggered: window.fail("timed out") }
}
