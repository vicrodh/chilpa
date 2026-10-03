import QtQuick
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Components"
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
    SourceDiscoveryControls { width: Metrics.confirmWidth; about: ({plugin:"preview",tab:"home",genres:[],shelves:[{id:"releases",title:"Releases",items:[]}]}) }
    SourceRecordDetails { width: Metrics.confirmWidth }
    SourcePageHeader { width: Metrics.confirmWidth; info: ({title:"Artist",description:"Biography"}) }
    SourceShelf { width: Metrics.confirmWidth; section: ({title:"Releases",error:"Unavailable",key:""}) }
    SourcePaging { about: ({plugin:"preview",next:"next"}) }
    SourceLibraryControls { source: "preview section:albums" }
    StartupSplash { width: Metrics.confirmWidth; height: Metrics.panesHeight }
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
