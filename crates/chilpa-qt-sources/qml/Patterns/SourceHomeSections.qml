import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Column {
    id: home
    property int refresh: 0
    readonly property var sections: {
        const sources = JSON.parse(Sources.homeLibraries(Library.remoteRev + Library.rev + Sources.revision + home.refresh) || "[]")
        const rows = []
        for (const source of sources) {
            const page = JSON.parse(Sources.page(source.id + " native:discover:home", Library.remoteRev) || "{}")
            let preferences = []
            try { preferences = JSON.parse(SettingsModel.kept("ui.source." + source.id + ".discover.home") || "[]") } catch (_) {}
            const available = page.shelves || []
            const sections = preferences.map(preference => Object.assign({}, available.find(section => section.id === preference.id) || {}, preference))
                .filter(section => available.some(found => found.id === section.id))
                .concat(available.filter(section => !preferences.some(preference => preference.id === section.id)))
            for (const section of sections) {
                if (section.enabled === false || !(section.items || []).length) continue
                rows.push(Object.assign({}, section, {title: source.name + " · " + section.title}))
            }
        }
        return rows
    }
    Repeater {
        model: home.sections
        SourceShelf { required property var modelData; width: home.width; section: modelData }
    }
}
