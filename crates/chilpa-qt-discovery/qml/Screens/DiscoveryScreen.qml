import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.artwork
import app.chilpa.discovery
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns" as ArtistUi
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns" as ControlsUi
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Patterns"
import "../Components"

Item {
    id: discoveryPage
    property string mode: "genres"
    property string routeKey: ""
    function decode(key) { try { return JSON.parse(key || "{}") || {} } catch (_) { return {} } }
    readonly property var selected: decode(routeKey)
    readonly property bool detail: !!selected.kind
    readonly property bool work: selected.kind === "work"
    readonly property bool composer: selected.kind === "composer"
    readonly property bool genre: selected.kind === "genre"
    readonly property bool overview: genre && tab === 0
    readonly property string rootTitle: mode === "genres" ? I18n.tr("Genres", I18n.rev) : mode === "composers" ? I18n.tr("Composers", I18n.rev) : I18n.tr("Compositions", I18n.rev)
    property int tab: 0
    property real restoredPosition: 0
    property var positions: ({})
    property bool restoring: false
    property bool playSelection: false
    property bool sourceOverride: false
    readonly property string memory: "discovery." + mode + "." + routeKey
    readonly property var about: decode(results.document)
    // The reader resolves inherited sources from the album. Its answer updates the picker without changing the requested route.
    readonly property string collectionKey: mode !== "genres" || selected.albumScope === true && !sourceOverride ? routeKey : JSON.stringify(Object.assign({}, selected, {excluded_sources:find.excludedSources}, selected.albumScope ? {albumScope:false} : {}))
    readonly property var breadcrumb: (selected.trail || []).length > 1 ? selected.trail : about.breadcrumb || selected.trail || []
    readonly property bool listing: !detail || composer && tab === 0 || work && tab === 0 || genre && tab === 4
    readonly property string queryKind: work ? "recordings" : mode
    readonly property bool needsMetadata: !selected.recording && (listing || genre)
    readonly property string queryIdentity: JSON.stringify([queryKind,collectionKey,find.text,find.order,find.scope])
    property string requestedIdentity: ""
    readonly property bool queryPending: needsMetadata && (requestedIdentity !== queryIdentity || results.loading || !Array.isArray(about.items))
    readonly property string failure: !queryPending && needsMetadata && about.error ? about.error : content.item?.error || ""
    readonly property bool waiting: failure === "" && (queryPending || !content.item || content.item.ready === false)
    readonly property Component contentComponent: selected.recording ? recordingParts : overview ? genreOverview : listing ? mode === "compositions" || composer || work ? workTable : navigationList : composer || work ? tab === 1 ? albums : tracks : tab === 1 ? albums : tab === 2 ? artists : tracks
    readonly property var choices: composer ? [I18n.tr("Compositions", I18n.rev), I18n.tr("Albums", I18n.rev), I18n.tr("Tracks", I18n.rev)]
        : work ? [I18n.tr("Recordings", I18n.rev), I18n.tr("Albums", I18n.rev)]
        : [I18n.tr("Overview", I18n.rev), I18n.tr("Albums", I18n.rev), I18n.tr("Artists", I18n.rev), I18n.tr("Tracks", I18n.rev), I18n.tr("Subgenres", I18n.rev)]
    function restore() {
        const state = Visit.recall("discovery.visits", {})[memory] || {}
        positions = state.positions || {}; tab = state.tab || 0
        if (genre && state.version !== 2 && Object.keys(state).length > 0) {
            const shifted = {}
            for (const key of Object.keys(positions)) shifted[Number(key) + 1] = positions[key]
            positions = shifted; tab++
        }
        find.text = state.text || ""; find.order = state.order || "name"; find.scope = state.scope || "all"
        sourceOverride = state.sourceOverride === true
        find.excludedSources = selected.excluded_sources || state.excludedSources || []
        restoredPosition = positions[tab] || state.position || 0; restoring = restoredPosition > 0
        schedule()
    }
    function remember() {
        // At most 64 visits per window; long route keys remain bounded by the facade.
        const previous = Visit.recall("discovery.visits", {})
        const next = {}
        for (const key of Object.keys(previous).filter(key => key !== memory).slice(-63)) next[key] = previous[key]
        next[memory] = {version:2, tab:tab, order:find.order, scope:find.scope, sourceOverride:sourceOverride, excludedSources:find.excludedSources, text:find.text, positions:positions, position:positions[tab] || 0}
        Visit.keep("discovery.visits", next)
    }
    function changeTab(next) {
        if (tab === next) return
        tab = next; restoredPosition = positions[next] || 0; restoring = restoredPosition > 0
        remember(); schedule(); Qt.callLater(restoreViewport)
    }
    function restoreViewport() {
        const view = content.item?.scrollView
        if (!view || !content.item.ready || !restoring) return
        view.contentY = Math.max(0, Math.min(restoredPosition, view.contentHeight - view.height))
        restoring = false
    }
    function updateContent() {
        if (content) content.replace(contentComponent, visible)
    }
    onContentComponentChanged: updateContent()
    Connections {
        target: content.item?.scrollView || null
        function onContentYChanged() { if (!discoveryPage.restoring && !content.retiring) discoveryPage.positions[discoveryPage.tab] = target.contentY }
        function onContentHeightChanged() { Qt.callLater(discoveryPage.restoreViewport) }
    }
    Connections { target: content.item; function onReadyChanged() { Qt.callLater(discoveryPage.restoreViewport) } }
    Component.onCompleted: { restore(); updateContent(); settle.triggered() }
    Component.onDestruction: remember()
    onRouteKeyChanged: { playSelection = false; sourceOverride = false; tab = 0; find.text = ""; find.excludedSources = decode(routeKey).excluded_sources || []; if (results) results.cancel(); schedule() }
    function schedule() { if (visible) settle.restart() }
    function retry() {
        if (needsMetadata && about.error) { requestedIdentity = ""; schedule() }
        else if (content.item?.retry) content.item.retry()
    }
    onVisibleChanged: {
        if (visible) { restoredPosition = positions[tab] || 0; restoring = restoredPosition > 0 }
        updateContent()
        if (visible) schedule(); else { settle.stop(); results.cancel() }
    }
    Connections { target: Library; function onContentRevChanged() { discoveryPage.schedule() } function onRemoteRevChanged() { discoveryPage.schedule() } }
    Connections { target: Artwork; function onRevChanged() { discoveryPage.schedule() } }
    DiscoveryModel {
        id: results
        objectName: "discoveryModel"
        onDocumentChanged: {
            const document = discoveryPage.decode(results.document)
            if (find && Array.isArray(document.excluded_sources) && discoveryPage.decode(discoveryPage.routeKey).albumScope === true && !discoveryPage.sourceOverride) find.excludedSources = document.excluded_sources
        }
    }
    Timer {
        id: settle
        interval: Motion.searchSettle
        onTriggered: if (discoveryPage.visible && discoveryPage.needsMetadata) {
            discoveryPage.requestedIdentity = discoveryPage.queryIdentity
            const key = discoveryPage.collectionKey
            results.request(discoveryPage.queryKind, key, find.text, JSON.stringify({order:find.order,scope:find.scope,excluded_sources:discoveryPage.decode(key).excluded_sources}), Library.contentRev + Library.remoteRev + Artwork.rev)
        }
    }
    function open(entry) {
        if (entry.kind === "recording") {
            const next = Object.assign({}, selected, {recording:entry.recording, recordingTitle:entry.name, performer:entry.artist, album:entry.album})
            Navigation.goKey("compositions", JSON.stringify(next))
            return
        }
        const next = entry.kind === "genre" && entry.key ? Object.assign({}, decode(entry.key)) : {kind:entry.kind, name:entry.name, work:entry.work || "", composers:entry.composers || []}
        if (entry.kind === "genre") {
            next.trail = (breadcrumb || []).concat([entry.name]); next.excluded_sources = find.excludedSources
            if (next.albumScope !== undefined) next.albumScope = false
        }
        Navigation.goKey(entry.kind === "genre" ? "genres" : entry.kind === "composer" ? "composers" : "compositions", JSON.stringify(next))
    }
    Column {
        id: heading
        x: Metrics.contentX; y: Metrics.contentTop
        width: discoveryPage.width - Metrics.contentX * 2 // derived: content margins
        spacing: Metrics.gap12
        DiscoveryBreadcrumb {
            width: parent.width
            visible: discoveryPage.detail
            title: discoveryPage.rootTitle; mode: discoveryPage.mode; trail: discoveryPage.breadcrumb; excludedSources: find.excludedSources
        }
        DiscoveryTools {
            id: find
            width: parent.width
            title: discoveryPage.detail ? discoveryPage.selected.name || discoveryPage.rootTitle : discoveryPage.rootTitle
            mode: discoveryPage.mode; listing: discoveryPage.listing; work: discoveryPage.work; detail: discoveryPage.detail
            overview: discoveryPage.overview; recording: !!discoveryPage.selected.recording
            tracks: discoveryPage.detail && discoveryPage.tab === (discoveryPage.genre ? 3 : 2)
            sources: discoveryPage.about.sources || []
            onSourcesChosen: excluded => {
                discoveryPage.sourceOverride = true
                find.excludedSources = excluded; discoveryPage.playSelection = false; discoveryPage.positions = ({}); discoveryPage.restoring = false
                if (content.item?.scrollView) content.item.scrollView.contentY = 0
                discoveryPage.remember(); discoveryPage.schedule()
            }
            onQueryChanged: { discoveryPage.restoring = false; discoveryPage.positions[discoveryPage.tab] = 0; discoveryPage.schedule() }
        }
        DiscoveryActions {
            width: parent.width
            mode: discoveryPage.mode
            selected:discoveryPage.selected; about:discoveryPage.about; choices:discoveryPage.choices
            tab:discoveryPage.tab; overview:discoveryPage.overview; loading:discoveryPage.queryPending
            onOpenRequested: entry => discoveryPage.open(entry)
            onTabRequested: tab => discoveryPage.changeTab(tab)
        }
        ControlsUi.CollectionPlayback {
            visible: discoveryPage.genre && (discoveryPage.tab === 1 || discoveryPage.tab === 3)
            availableWidth: heading.width
            model: visible ? content.item?.collection || null : null
            available: !!model && !content.retiring && !discoveryPage.waiting && content.item.playable
            filtered: find.excludedSources.length > 0 || (showRandomAlbum && find.text.trim() !== "") || content.item?.sheetFiltered === true
            showRandomAlbum: discoveryPage.tab === 1
            scope: JSON.stringify([discoveryPage.collectionKey, discoveryPage.tab, find.text, find.order])
            label: discoveryPage.selected.name || discoveryPage.rootTitle
        }
    }
    RetiringLoader {
        id: content
        objectName: "discoveryContent"
        visible: !discoveryPage.waiting && discoveryPage.failure === ""
        anchors.left: heading.left; anchors.right: heading.right; anchors.top: heading.bottom; anchors.topMargin: Metrics.gap20
        anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.floatingPlayerClearance // docked, the plane already ends above the player
        onLoaded: Qt.callLater(discoveryPage.restoreViewport)
    }
    Component {
        id: genreOverview
        GenreOverview {
            key:discoveryPage.collectionKey; filter:find.text; order:find.order; name:discoveryPage.selected.name || ""; about:discoveryPage.about; loading:discoveryPage.queryPending; scrollbarOverlay:discoveryPage
            artistsContent: Component {
                Item {
                    AlbumGridModel { id:people; kind:"discoveryArtists"; key:discoveryPage.collectionKey; filter:find.text; order:find.order; columns:grid.columns; contentRev:Library.contentRev + Library.remoteRev }
                    ArtistUi.ArtistCardGrid { id:grid; anchors.fill:parent; artists:people; visible:people.ready && people.error === ""; flow:GridView.FlowTopToBottom; flickableDirection:Flickable.HorizontalFlick }
                    ShelfSkeleton { anchors.fill:parent; visible:!people.ready; round:true }
                    Column {
                        anchors.centerIn:parent; spacing:Metrics.gap12
                        Text { text:people.error || I18n.tr("No artists in this selection", I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.body }
                        PillButton { anchors.horizontalCenter:parent.horizontalCenter; visible:people.error !== ""; text:I18n.tr("Retry", I18n.rev); onClicked:people.retry() }
                        visible:people.ready && (people.error !== "" || people.albums === 0)
                    }
                }
            }
            onActivated: entry => discoveryPage.open(entry)
            onBrowseRequested: section => { discoveryPage.playSelection = false; discoveryPage.changeTab(({albums:1,artists:2,tracks:3,subgenres:4})[section] || 0) }
            onPlayRequested: { discoveryPage.playSelection = true; discoveryPage.changeTab(3) }
        }
    }
    Component {
        id: navigationList
        DiscoveryIndex {
            kind: discoveryPage.mode; scrollbarOverlay: discoveryPage
            ready: !results.loading && Array.isArray(discoveryPage.about.items)
            dataSource: results
            onActivated: entry => discoveryPage.open(entry)
        }
    }
    Component {
        id: workTable
        DiscoveryWorkTable {
            recordings: discoveryPage.work; scrollbarOverlay: discoveryPage
            ready: !results.loading && Array.isArray(discoveryPage.about.items)
            dataSource: results
            onActivated: entry => discoveryPage.open(entry)
        }
    }
    Component {
        id: albums
        DiscoveryAlbums { key:discoveryPage.collectionKey; filter:find.text; order:find.order.replace("name","title"); scrollbarOverlay:discoveryPage }
    }
    Component {
        id: artists
        DiscoveryArtists {
            key:discoveryPage.collectionKey; filter:find.text; order:find.order
            cards: Component { ArtistUi.ArtistCardGrid { artists:parent.artists } }
        }
    }
    Component { id: tracks; DiscoveryTracks { key: discoveryPage.collectionKey; autoPlay:discoveryPage.playSelection; showLegacyActions: !discoveryPage.genre; scrollbarOverlay:discoveryPage; onPlayed:discoveryPage.playSelection = false } }
    Component { id: recordingParts; DiscoveryTracks { key: discoveryPage.collectionKey; recording: true; scrollbarOverlay:discoveryPage } }
    DiscoveryStatus {
        anchors.fill:content; loading:discoveryPage.waiting; failure:discoveryPage.failure
        empty:discoveryPage.listing && !discoveryPage.selected.recording && (discoveryPage.about.items || []).length === 0
        emptyText: find.text !== "" ? I18n.tr("No matches. Try another search.", I18n.rev)
                : discoveryPage.mode === "genres" ? I18n.tr("No genres with albums in this selection", I18n.rev)
                : discoveryPage.mode === "composers" && !discoveryPage.composer ? I18n.tr("No composer credits yet. Add composers in the metadata editor.", I18n.rev)
                : I18n.tr("No work tags in this selection. Composer credits are still available under Albums and Tracks.", I18n.rev)
        onRetryRequested:discoveryPage.retry()
    }
}
