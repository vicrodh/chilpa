import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.discovery
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns" as ArtistUi
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Patterns"

Item {
    id: discoveryPage
    property string mode: "genres"
    property string routeKey: ""
    function decode(key) { try { return JSON.parse(key || "{}") || {} } catch (_) { return {} } }
    readonly property var selected: decode(routeKey)
    readonly property bool detail: !!selected.kind
    readonly property bool work: selected.kind === "work"
    readonly property bool composer: selected.kind === "composer"
    readonly property string rootTitle: mode === "genres" ? I18n.tr("Genres", I18n.rev) : mode === "composers" ? I18n.tr("Composers", I18n.rev) : I18n.tr("Compositions", I18n.rev)
    property int tab: 0
    property int number: 0
    property real restoredPosition: 0
    property var positions: ({})
    property bool restoring: false
    readonly property string memory: "discovery." + mode + "." + routeKey
    readonly property var about: decode(results.document)
    readonly property var breadcrumb: (selected.trail || []).length > 1 ? selected.trail : about.breadcrumb || selected.trail || []
    readonly property bool listing: !detail || composer && tab === 0 || work && tab === 0 || selected.kind === "genre" && tab === 3
    readonly property string queryKind: work ? "recordings" : mode
    readonly property var choices: composer ? [I18n.tr("Compositions", I18n.rev), I18n.tr("Albums", I18n.rev), I18n.tr("Tracks", I18n.rev)]
        : work ? [I18n.tr("Recordings", I18n.rev), I18n.tr("Albums", I18n.rev)]
        : [I18n.tr("Albums", I18n.rev), I18n.tr("Artists", I18n.rev), I18n.tr("Tracks", I18n.rev), I18n.tr("Subgenres", I18n.rev)]
    function restore() {
        const state = Visit.recall("discovery.visits", {})[memory] || {}
        positions = state.positions || {}; tab = state.tab || 0; find.text = state.text || ""; number = state.page || 0
        restoredPosition = positions[tab] || state.position || 0; restoring = restoredPosition > 0
        schedule()
    }
    function remember() {
        // At most 64 visits per window; long route keys remain bounded by the facade.
        const previous = Visit.recall("discovery.visits", {})
        const next = {}
        for (const key of Object.keys(previous).filter(key => key !== memory).slice(-63)) next[key] = previous[key]
        next[memory] = {tab:tab, page:number, text:find.text, positions:positions, position:positions[tab] || 0}
        Visit.keep("discovery.visits", next)
    }
    function changeTab(next) {
        if (tab === next) return
        tab = next; number = 0; restoredPosition = positions[next] || 0; restoring = restoredPosition > 0
        remember(); schedule(); Qt.callLater(restoreViewport)
    }
    function restoreViewport() {
        const view = content.item?.scrollView
        if (!view || !content.item.ready || !restoring) return
        view.contentY = Math.max(0, Math.min(restoredPosition, view.contentHeight - view.height))
        restoring = false
    }
    Connections {
        target: content.item?.scrollView || null
        function onContentYChanged() { if (!discoveryPage.restoring) discoveryPage.positions[discoveryPage.tab] = target.contentY }
        function onContentHeightChanged() { Qt.callLater(discoveryPage.restoreViewport) }
    }
    Connections { target: content.item; function onReadyChanged() { Qt.callLater(discoveryPage.restoreViewport) } }
    Component.onCompleted: { restore(); settle.triggered() }
    Component.onDestruction: remember()
    onRouteKeyChanged: { tab = 0; number = 0; find.text = ""; schedule() }
    function schedule() { if (visible) settle.restart() }
    onVisibleChanged: { if (visible) schedule(); else { settle.stop(); results.cancel() } }
    Connections { target: Library; function onContentRevChanged() { discoveryPage.schedule() } function onRemoteRevChanged() { discoveryPage.schedule() } }
    DiscoveryModel { id: results; objectName: "discoveryModel" }
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: if (discoveryPage.visible && !discoveryPage.selected.recording && (discoveryPage.listing || discoveryPage.selected.kind === "genre")) results.request(discoveryPage.queryKind, discoveryPage.routeKey, discoveryPage.selected.kind === "genre" && discoveryPage.tab !== 3 ? "" : find.text, discoveryPage.number, Library.contentRev + Library.remoteRev)
    }
    function open(entry) {
        if (entry.kind === "recording") {
            const next = Object.assign({}, selected, {recording:entry.recording, recordingTitle:entry.name, performer:entry.artist, album:entry.album})
            Navigation.goKey("compositions", JSON.stringify(next))
            return
        }
        const next = {kind:entry.kind, name:entry.name, work:entry.work || "", composers:entry.composers || []}
        if (entry.kind === "genre") next.trail = (breadcrumb || []).concat([entry.name])
        Navigation.goKey(entry.kind === "genre" ? "genres" : entry.kind === "composer" ? "composers" : "compositions", JSON.stringify(next))
    }
    Column {
        id: heading
        x: Metrics.contentX; y: Metrics.contentTop
        width: discoveryPage.width - Metrics.contentX * 2 // derived: content margins
        spacing: Metrics.gap12
        Flow {
            width: parent.width; spacing: Metrics.gap4
            visible: discoveryPage.detail
            PillButton { text: discoveryPage.rootTitle; kind: "ghost"; onClicked: Navigation.goKey(discoveryPage.mode, "") }
            Repeater {
                model: discoveryPage.breadcrumb
                PillButton {
                    required property string modelData
                    required property int index
                    kind: "ghost"; text: modelData; enabled: index < discoveryPage.breadcrumb.length - 1
                    onClicked: Navigation.goKey("genres", JSON.stringify({kind:"genre",name:modelData,trail:discoveryPage.breadcrumb.slice(0,index + 1)}))
                }
            }
        }
        ScreenHead {
            id: head
            width: parent.width
            title: discoveryPage.detail ? discoveryPage.selected.name || discoveryPage.rootTitle : discoveryPage.rootTitle
            flexible: find
            SearchField {
                id: find
                objectName: "discoverySearch"
                maximumLength: Metrics.discoverySearchChars
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, Math.min(Metrics.screenSearch, head.flexibleRoom))
                visible: !discoveryPage.selected.recording && !(discoveryPage.detail && discoveryPage.tab === 2)
                placeholder: discoveryPage.work ? I18n.tr("Filter recordings", I18n.rev) : I18n.tr("Search your library", I18n.rev)
                onTextChanged: { discoveryPage.number = 0; discoveryPage.schedule() }
                onEscaped: text = ""
            }
        }
        Text {
            width: parent.width
            text: discoveryPage.work ? I18n.tr("Available parts, grouped by album and performer. Work tags do not confirm a complete recording.", I18n.rev)
                : discoveryPage.composer ? I18n.tr("Explore works, recordings and albums credited to this composer.", I18n.rev)
                : discoveryPage.mode === "genres" ? I18n.tr("Follow a style into its subgenres, or search for one directly.", I18n.rev)
                : discoveryPage.mode === "composers" ? I18n.tr("The people behind the music in your library.", I18n.rev)
                : I18n.tr("Works identified by work and composer tags. Recordings keep their album and performer context.", I18n.rev)
            wrapMode: Text.WordWrap; textFormat: Text.PlainText
            font.pointSize: Type.body; color: Theme.color.textSecondary
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            visible: discoveryPage.work && (discoveryPage.selected.composers || []).length > 0
            Repeater { model: discoveryPage.selected.composers || []; PillButton { required property string modelData; text: modelData; maximumWidth: parent.width; onClicked: discoveryPage.open({kind:"composer",name:modelData}) } }
        }
        Flow {
            objectName: "discoverySubgenres"
            width: parent.width; spacing: Metrics.gap8
            visible: discoveryPage.detail && !discoveryPage.composer && !discoveryPage.work && discoveryPage.tab !== 3 && !results.loading
            Repeater { model: (discoveryPage.about.items || []).slice(0, Metrics.discoveryPreview); PillButton { required property var modelData; text: modelData.name; maximumWidth: parent.width; onClicked: discoveryPage.open(modelData) } }
        }
        Row {
            visible: discoveryPage.selected.kind === "genre" && discoveryPage.tab !== 3 && (discoveryPage.about.total || 0) > Metrics.discoveryPreview
            PillButton { text: I18n.tr("All subgenres", I18n.rev); kind: "ghost"; onClicked: { discoveryPage.changeTab(3) } }
        }
        Flow {
            width: parent.width
            visible: !!discoveryPage.selected.recording
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("All recordings", I18n.rev); kind: "ghost"; onClicked: Navigation.goKey("compositions", JSON.stringify(Object.assign({}, discoveryPage.selected, {recording:"",recordingTitle:"",performer:"",album:0}))) }
            PillButton { objectName: "discoveryRecordingAlbum"; maximumWidth: parent.width; text: discoveryPage.selected.recordingTitle || ""; visible: (discoveryPage.selected.album || 0) !== 0; onClicked: Navigation.go("album", discoveryPage.selected.album) }
            Text { width: Math.min(implicitWidth, parent.width); elide: Text.ElideRight; text: discoveryPage.selected.performer || ""; textFormat: Text.PlainText; font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
        SegmentedControl {
            objectName: "discoveryTabs"
            visible: discoveryPage.detail && !discoveryPage.selected.recording
            options: discoveryPage.choices.map(text => ({text:text}))
            current: discoveryPage.tab
            onActivated: index => discoveryPage.changeTab(index)
        }
        Row {
            spacing: Metrics.gap8
            visible: discoveryPage.composer
            PillButton {
                text: I18n.tr("About this composer", I18n.rev); kind: "ghost"
                onClicked: {
                    Visit.keep("artist.aboutIntent", {name:discoveryPage.selected.name || "", at:Date.now()})
                    Navigation.openArtist(discoveryPage.selected.name || "")
                }
            }
        }
    }
    Loader {
        id: content
        objectName: "discoveryContent"
        anchors.left: heading.left; anchors.right: heading.right; anchors.top: heading.bottom; anchors.topMargin: Metrics.gap20
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.contentBottom
        active: discoveryPage.visible
        onLoaded: Qt.callLater(discoveryPage.restoreViewport)
        sourceComponent: discoveryPage.selected.recording ? recordingParts : discoveryPage.listing ? navigationList : discoveryPage.composer || discoveryPage.work ? discoveryPage.tab === 1 ? albums : tracks : discoveryPage.tab === 0 ? albums : discoveryPage.tab === 1 ? artists : tracks
    }
    Component {
        id: navigationList
        DiscoveryIndex {
            ready: !results.loading && Array.isArray(discoveryPage.about.items)
            entries: results.loading ? [] : discoveryPage.about.items || []; total: discoveryPage.about.total || 0; page: discoveryPage.number; loading: results.loading
            onActivated: entry => discoveryPage.open(entry)
            onPageRequested: number => { discoveryPage.number = number; discoveryPage.positions[discoveryPage.tab] = 0; discoveryPage.restoring = false; discoveryPage.schedule() }
        }
    }
    Component {
        id: albums
        Item {
            readonly property alias scrollView: grid.scrollView
            readonly property bool ready: records.ready
            AlbumGridModel { id: records; kind: "discovery"; key: discoveryPage.routeKey; filter: find.text; columns: grid.columns; contentRev: Library.contentRev + Library.remoteRev }
            AlbumGrid { id: grid; anchors.fill: parent; model: records; onOpen: albumId => Navigation.go("album", albumId) }
            Text { anchors.centerIn: parent; visible: records.ready && records.albums === 0; text: I18n.tr("No albums in this selection", I18n.rev); font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
    }
    Component {
        id: artists
        Item {
            readonly property alias scrollView: grid
            readonly property bool ready: people.ready
            AlbumGridModel { id: people; kind: "discoveryArtists"; key: discoveryPage.routeKey; filter: find.text; columns: grid.columns; contentRev: Library.contentRev + Library.remoteRev }
            ArtistUi.ArtistCardGrid { id: grid; anchors.fill: parent; artists: people }
            Text { anchors.centerIn: parent; visible: people.ready && people.albums === 0; text: I18n.tr("No artists in this selection", I18n.rev); font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
    }
    Component { id: tracks; DiscoveryTracks { key: discoveryPage.routeKey } }
    Component { id: recordingParts; DiscoveryTracks { key: discoveryPage.routeKey } }
    Column {
        anchors.centerIn: content
        width: Math.min(content.width, Metrics.settingsMaxWidth)
        spacing: Metrics.gap12
        visible: discoveryPage.listing && !discoveryPage.selected.recording && (results.loading || !!discoveryPage.about.error || (discoveryPage.about.items || []).length === 0)
        Text {
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; textFormat: Text.PlainText
            text: results.loading ? I18n.tr("Loading…", I18n.rev) : discoveryPage.about.error ? I18n.tr("Could not read the library", I18n.rev)
                : find.text !== "" ? I18n.tr("No matches. Try another search.", I18n.rev)
                : discoveryPage.mode === "genres" ? I18n.tr("No genre tags yet. Add genres in the metadata editor to start exploring.", I18n.rev)
                : discoveryPage.mode === "composers" && !discoveryPage.composer ? I18n.tr("No composer credits yet. Add composers in the metadata editor.", I18n.rev)
                : I18n.tr("No work tags in this selection. Composer credits are still available under Albums and Tracks.", I18n.rev)
            font.pointSize: Type.body; color: Theme.color.textSecondary
        }
        PillButton { anchors.horizontalCenter: parent.horizontalCenter; visible: !!discoveryPage.about.error; text: I18n.tr("Retry", I18n.rev); onClicked: discoveryPage.schedule() }
    }
}
