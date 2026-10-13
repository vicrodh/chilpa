import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The sources' shelves that join the Home, each with an id that stays
// across runs ("<source>:<shelf>"). Shown here, or handed to a page that
// places them among its own (`rendered: false`).
Column {
    id: home
    property int refresh: 0
    property bool rendered: true
    property bool deferred: false
    property var resolved: []
    readonly property var sources: JSON.parse(Sources.homeLibraries(Library.remoteRev + Library.rev + Sources.revision + home.refresh) || "[]")
    readonly property var sections: deferred ? resolved : sources.flatMap(source => sectionsOf(source))
    function sectionsOf(source) {
        const rows = []
        const page = JSON.parse(Sources.page(source.id + " native:discover:home", Library.remoteRev) || "{}")
        let preferences = []
        try { preferences = JSON.parse(SettingsModel.kept("ui.source." + source.id + ".discover.home") || "[]") } catch (_) {}
        const available = page.shelves || []
        const sections = preferences.map(preference => Object.assign({}, available.find(section => section.id === preference.id) || {}, preference))
            .filter(section => available.some(found => found.id === section.id))
            .concat(available.filter(section => !preferences.some(preference => preference.id === section.id)))
        for (const section of sections) {
            rows.push(Object.assign({}, section, {title: source.icon ? section.title : source.name + " · " + section.title,
                enabled: source.enabled === true && section.enabled !== false,
                sourceIcon: source.icon || "", sourceId: source.id, sourceName: source.name,
                homeId: source.id + ":" + section.id}))
        }
        return rows
    }
    onSourcesChanged: {
        resolved = []
        if (deferred) { step.index = 0; step.restart() }
    }
    onDeferredChanged: {
        if (deferred) { resolved = []; step.index = 0; step.restart() }
        else step.stop()
    }
    Timer {
        id: step
        property int index: 0
        interval: 16
        repeat: true
        onTriggered: {
            if (index >= home.sources.length) { stop(); return }
            home.resolved = home.resolved.concat(home.sectionsOf(home.sources[index++]))
        }
    }
    Repeater {
        model: home.rendered ? home.sections.filter(section => section.enabled !== false && ((section.items || []).length || section.error)) : []
        SourceShelf { required property var modelData; width: home.width; section: modelData }
    }
}
