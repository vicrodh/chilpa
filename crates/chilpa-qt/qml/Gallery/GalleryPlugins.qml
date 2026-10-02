import QtQuick
import "../Theme"
import "../Components"
import "../Patterns"

GallerySection {
    title: "Plugins"
    QtObject {
        id: sample
        function pluginsJson() { return JSON.stringify({catalog: [], installed: [], repositories: []}) }
        function pluginCommand(action, input) {}
    }
    PluginValidation { width: Metrics.confirmWidth; phase: "passed" }
    PluginRow { width: Metrics.confirmWidth; plugin: ({name: "Catalog source", light: "yellow"}) }
    PluginSettingsForm {
        width: Metrics.confirmWidth
        document: ({form: {fields: [{key: "user", label: "Account", control: {type: "text"}}], actions: []}, values: {}})
    }
    PluginIntegration { width: Metrics.confirmWidth; plugin: ({id: "guest", tint: false}) }
    PluginDetails { manager: preview }
    PluginRepositories { manager: preview }
    PluginManager { id: preview; width: Metrics.confirmWidth; height: Metrics.panesHeight; page: sample }
}
