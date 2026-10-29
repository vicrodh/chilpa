import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/SectionChoices.js" as SectionChoices
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I keep the last descriptors while refreshing one source per frame.
// Returning to Home reserves the same slots before any plugin is queried.
Column {
    id: home
    objectName: "homeSourceSections"
    property int refresh: 0
    property bool rendered: true
    property bool active: true
    // Explicit module choices and the source currently inspected in Customize
    // can be read even when its default Home integration is off.
    property var selectedSources: []
    property string browsingSource: ""
    property bool ready: false
    property var documents: Visit.recall("home.source.documents", ({}))
    property var sections: Visit.recall("home.source.sections", [])
    property var loadedSources: ({})
    readonly property string sourceDocument: active
        ? Sources.homeLibraries(Library.rev + Sources.revision + home.refresh) || "[]"
        : JSON.stringify(Visit.recall("home.source.list", []))
    readonly property var sources: JSON.parse(sourceDocument)
    onSourcesChanged: scheduled.restart()
    onRefreshChanged: scheduled.restart()
    onActiveChanged: scheduled.restart()
    onSelectedSourcesChanged: scheduled.restart()
    onBrowsingSourceChanged: scheduled.restart()
    Component.onCompleted: scheduled.restart()
    Connections {
        target: Library
        function onRemoteRevChanged() {
            if (home.active && !scheduled.running) home.updateLive()
        }
    }
    Timer { id: scheduled; interval: 0; onTriggered: home.schedule() }
    function schedule() {
        if (!active) { step.stop(); return }
        Visit.keep("home.source.list", sources)
        loadedSources = ({})
        step.index = 0
        ready = sources.length === 0
        publish()
        if (!ready) step.restart()
    }
    function publish() {
        let rows = []
        for (const source of sources)
            rows = rows.concat((documents[source.id] || []).map(section =>
                Object.assign({}, section, { enabled: source.enabled === true && section.sourceEnabled !== false })))
        sections = rows
        Visit.keep("home.source.sections", sections)
    }
    function updateLive() {
        for (const source of sources) {
            if (!loadedSources[source.id]) resolve(source)
            else if ((documents[source.id] || []).some(section => section.live === true)) resolve(source, true)
        }
    }
    function isLive(section) {
        if (section.live !== undefined) return section.live === true
        // Older documents expose listening history through these generic roles.
        // A personal page also contains editorial shelves, which stay still.
        return section.personal === true && /^(recent|recent_albums|recent_playlists|recent_tracks|recently_played|recently-played|continue|history)$/.test(section.id || "")
    }
    function resolve(source, liveOnly) {
        if (!active || (source.enabled !== true && !selectedSources.includes(source.id) && browsingSource !== source.id)) return
        const page = JSON.parse(Sources.page(source.id + " native:discover:home", Library.remoteRev) || "{}")
        if (page.loading === true && !(page.shelves || []).length) return
        let preferences = []
        try { preferences = JSON.parse(SettingsModel.kept("ui.source." + source.id + ".discover.home") || "[]") } catch (_) {}
        const available = (page.shelves || []).map(section => Object.assign({}, section, {size:0}))
        const ordered = SectionChoices.order(available, preferences)
        const previous = documents[source.id] || []
        let next = ordered.map(section => Object.assign({}, section, {
            title: source.icon ? section.title : source.name + " · " + section.title,
            live: isLive(section),
            sourceEnabled: section.enabled !== false, sourceIcon: source.icon || "", sourceId: source.id,
            sourceName: source.name, homeId: source.id + ":" + section.id
        }))
        if (liveOnly) next = previous.map(section => {
            if (section.live !== true) return section
            return next.find(candidate => candidate.homeId === section.homeId) || section
        })
        loadedSources[source.id] = page.loading !== true
        if (JSON.stringify(previous) === JSON.stringify(next)) return
        documents[source.id] = next
        Visit.keep("home.source.documents", documents)
        publish()
    }
    Timer {
        id: step
        property int index: 0
        interval: Motion.moduleStart
        repeat: true
        onTriggered: {
            if (!home.active || index >= home.sources.length) { stop(); home.ready = true; return }
            home.resolve(home.sources[index++])
            if (index >= home.sources.length) { stop(); home.ready = true }
        }
    }
    KeyedModel { id: renderedRows; keyField: "homeId"; items: home.rendered ? home.sections.filter(section => section.enabled !== false && ((section.items || []).length || section.error)) : [] }
    Repeater {
        model: renderedRows
        SourceShelf { required property var entry; active: home.active; width: home.width; section: entry; oneRow: true }
    }
}
