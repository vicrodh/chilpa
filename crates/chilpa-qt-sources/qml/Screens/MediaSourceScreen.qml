import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

Item {
    id: page
    readonly property var view: JSON.parse(Sources.snapshot || "{}")
    readonly property bool radio: view.medium === "radio"
    readonly property bool root: (view.crumbs || []).length === 0
    readonly property bool show: !!view.about && !radio
    readonly property string tab: view.media_tab || "home"
    property string query: ""
    property string episodeFilter: "all"
    property string season: ""
    property string episodeType: ""
    property bool compact: false
    property var quick: null
    onShowChanged: if (!show) { episodeFilter = "all"; season = ""; episodeType = "" }
    readonly property var entries: (view.entries || []).filter(e => !e.id.startsWith("chilpa:")).filter(e => episodeFilter === "all" || episodeFilter === "unplayed" && !e.played || episodeFilter === "progress" && e.progress > 0 && !e.played).filter(e => !season || ((e.presentation || {}).metadata || {}).season === season).filter(e => !episodeType || ((e.presentation || {}).metadata || {}).episode_type === episodeType)
    readonly property var seasons: Array.from(new Set((view.entries || []).map(e => ((e.presentation || {}).metadata || {}).season).filter(Boolean)))
    readonly property bool indexPage: root && ["genres", "countries", "categories"].indexOf(tab) >= 0
    readonly property var facet: (view.columns || []).find(c => c.id === (tab === "countries" ? "country" : radio ? "genre" : "category")) || ({entries: []})
    readonly property bool cards: !show && !compact && (radio ? root && tab === "home" : !entries.some(e => e.finite))
    function ask(action, id, text) { Sources.command(action, id || "", text || "") }
    function go(section) {
        quick = null; episodeFilter = "all"; season = ""; episodeType = ""
        ask("child", view.source, "media:" + section)
    }
    function open(entry) { if (entry.kind === "folder") { quick = null; ask("enter", entry.id) } else quick = entry }
    Timer { interval: Motion.poll; running: page.visible && (page.view.loading || (page.view.entries || []).some(e => e.sounding || e.resolving)); repeat: true; onTriggered: Sources.refresh() }
    Connections { target: Navigation; function onRouteKeyChanged() { if (Navigation.screen === "source") page.ask("open", Navigation.routeKey) } }
    Column {
        id: heading; x: Metrics.contentX; y: Metrics.contentTop; width: parent.width - Metrics.contentX * 2; spacing: Metrics.gap16 // derived: proportional space within the parent
        Text { text: page.root && page.tab === "explore" ? I18n.tr("Explore", I18n.rev) : page.view.name || ""; color: Theme.color.textPrimary; font.pointSize: Type.title; font.weight: Type.bold }
        Text { text: page.view.attribution || ""; color: Theme.color.textSecondary; font.pointSize: Type.meta }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Home", I18n.rev); kind: page.tab === "home" && page.root ? "primary" : "normal"; onClicked: page.go("home") }
            PillButton { text: I18n.tr("Explore", I18n.rev); kind: page.tab === "explore" ? "primary" : "normal"; onClicked: page.go("explore") }
            PillButton { text: page.radio ? I18n.tr("My stations", I18n.rev) : I18n.tr("Subscriptions", I18n.rev); kind: page.view.inSaved ? "primary" : "normal"; onClicked: page.go("saved") }
            PillButton { text: I18n.tr("Continue listening", I18n.rev); visible: !page.radio && (page.view.shortcuts || []).some(e => e.id === "chilpa:unfinished"); onClicked: page.go("continue") }
            PillButton { text: page.radio ? I18n.tr("Genres", I18n.rev) : I18n.tr("Categories", I18n.rev); onClicked: page.go(page.radio ? "genres" : "categories") }
            PillButton { visible: page.radio; text: I18n.tr("Countries", I18n.rev); onClicked: page.go("countries") }
            PillButton { text: page.compact ? I18n.tr("Rich", I18n.rev) : I18n.tr("Compact", I18n.rev); onClicked: page.compact = !page.compact }
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            SearchField { width: Math.min(Metrics.selectWidth, heading.width); placeholder: page.show ? I18n.tr("Search these episodes", I18n.rev) : page.radio ? I18n.tr("Search stations by name", I18n.rev) : I18n.tr("Search shows", I18n.rev); onTextChanged: { page.query = text; if (!page.indexPage) page.ask("filter", "", text) } onAccepted: if (!page.show && !page.indexPage) { page.ask("search", "", text) } }
            Repeater {
                model: page.root ? (page.view.columns || []).slice(0, page.tab === "explore" ? undefined : 1) : []
                SelectField {
                    required property var modelData
                    width: Math.min(Metrics.selectWidth, heading.width)
                    options: [modelData.title].concat(modelData.entries.map(e => e.title + (e.subtitle ? " · " + e.subtitle : "")))
                    values: [""].concat(modelData.entries.map(e => e.id)); value: modelData.picked || ""
                    onPicked: value => page.ask("pick", modelData.id, value)
                }
            }
            PillButton { visible: !page.root; text: I18n.tr("Back", I18n.rev); onClicked: { page.quick = null; page.ask("crumb", String(Math.max(0, (page.view.crumbs || []).length - 1))) } }
        }
        Text { visible: !!page.view.error; width: parent.width; wrapMode: Text.Wrap; text: page.view.error || ""; color: Theme.color.statusNegative; font.pointSize: Type.body }
        PillButton { visible: !!page.view.error; text: I18n.tr("Try again", I18n.rev); onClicked: page.ask("retry") }
    }
    ListView {
        id: scroll; x: Metrics.contentX; width: heading.width; anchors.top: heading.bottom; anchors.topMargin: Metrics.gap24; anchors.bottom: parent.bottom
        clip: true; spacing: Metrics.gap4
        model: page.cards || page.indexPage ? [] : page.entries
        delegate: MediaRow { required property var modelData; width: scroll.width; entry: modelData; compact: page.compact; onOpened: entry.kind === "folder" ? page.open(entry) : Navigation.goKey("sourceEntry", entry.key); onPlayed: page.ask("play", entry.id); onQueued: page.ask("queue", entry.id); onSaved: page.ask("save", entry.id); onHeard: page.ask("heard", entry.id) }
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        header: Column {
            id: content; width: scroll.width; bottomPadding: Metrics.gap24; spacing: Metrics.gap24
            MediaShelf {
                width: parent.width; visible: page.root && page.tab === "home" && entries.length > 0
                title: I18n.tr("Continue listening", I18n.rev); entries: page.view.continue_entries || []
                onOpened: entry => Navigation.goKey("sourceEntry", entry.key)
                onPlayed: entry => page.ask("play", entry.id)
                onSaved: entry => page.ask("save", entry.id)
                onMore: page.go("continue")
            }
            MediaShelf {
                width: parent.width; visible: page.root && page.tab === "home" && entries.length > 0
                title: page.radio ? I18n.tr("My stations", I18n.rev) : I18n.tr("Your shows", I18n.rev); station: page.radio; entries: page.view.saved_entries || []
                onOpened: entry => entry.kind === "folder" ? page.open(entry) : Navigation.goKey("sourceEntry", entry.key)
                onPlayed: entry => page.ask("play", entry.id)
                onSaved: entry => page.ask("save", entry.id)
                onMore: page.go("saved")
            }
            MediaShowHeader { width: parent.width; visible: page.show; entry: page.view.about || ({}); saved: page.view.placeSaved === true; onSubscribed: page.ask("save"); onPlayed: { const first = page.entries.find(e => e.playable); if (first) page.ask("play", first.id) } }
            Flow {
                width: parent.width; spacing: Metrics.gap8; visible: page.show
                Repeater { model: [{id:"all",name:I18n.tr("All", I18n.rev)},{id:"unplayed",name:I18n.tr("Unplayed", I18n.rev)},{id:"progress",name:I18n.tr("In progress", I18n.rev)}]; PillButton { required property var modelData; text: modelData.name; kind: page.episodeFilter === modelData.id ? "primary" : "normal"; onClicked: page.episodeFilter = modelData.id } }
                SelectField { options: [I18n.tr("Newest first", I18n.rev), I18n.tr("Oldest first", I18n.rev), I18n.tr("Not heard first", I18n.rev)]; values: ["", "oldest", "unheard"]; value: page.view.order || ""; onPicked: value => page.ask("order", value) }
                SelectField { options: [I18n.tr("All episode types", I18n.rev), I18n.tr("Full episodes", I18n.rev), I18n.tr("Trailers", I18n.rev), I18n.tr("Bonus", I18n.rev)]; values: ["", "full", "trailer", "bonus"]; value: page.episodeType; onPicked: value => page.episodeType = value }
                SelectField { visible: page.seasons.length > 0; options: [I18n.tr("All seasons", I18n.rev)].concat(page.seasons); values: [""].concat(page.seasons); value: page.season; onPicked: value => page.season = value }
            }
            Text { width: parent.width; visible: !page.show; text: page.indexPage ? page.tab === "countries" ? I18n.tr("Countries", I18n.rev) : page.radio ? I18n.tr("Genres", I18n.rev) : I18n.tr("Categories", I18n.rev) : page.view.inSaved ? page.radio ? I18n.tr("My stations", I18n.rev) : I18n.tr("Your shows", I18n.rev) : page.tab === "search" ? I18n.tr("Search results", I18n.rev) : page.radio ? I18n.tr("Stations", I18n.rev) : I18n.tr("Trending shows", I18n.rev); font.pointSize: Type.section; font.weight: Type.heading; color: Theme.color.textPrimary }
            Flow {
                width: parent.width; spacing: Metrics.gap12; visible: page.indexPage
                Repeater {
                    model: page.indexPage ? page.facet.entries.filter(entry => !page.query || entry.title.toLowerCase().indexOf(page.query.toLowerCase()) >= 0) : []
                    Rectangle {
                        required property var modelData
                        width: (content.width - Metrics.gap12 * (Math.max(2, Math.floor(content.width / Metrics.cardMinWidth)) - 1)) / Math.max(2, Math.floor(content.width / Metrics.cardMinWidth)) // derived: equal facet tiles within the viewport
                        height: Metrics.heroArt / 2; radius: Metrics.radiusCard; color: hover.hovered ? Theme.color.stateHover : Theme.color.fillField // derived: half a hero for a text-only tile
                        Column {
                            anchors.fill: parent; anchors.margins: Metrics.gap16; spacing: Metrics.gap6
                            Text { width: parent.width; text: modelData.title; wrapMode: Text.Wrap; color: Theme.color.textPrimary; font.pointSize: Type.body; font.weight: Type.semibold }
                            Text { width: parent.width; text: modelData.subtitle || ""; color: Theme.color.textSecondary; font.pointSize: Type.meta; elide: Text.ElideRight }
                        }
                        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: { const column = page.facet.id; page.go("explore"); page.ask("pick", column, modelData.id) } }
                    }
                }
            }
            MediaGrid {
                width: parent.width; visible: page.cards && !page.indexPage
                entries: visible ? page.entries : []; station: page.radio; selected: page.quick
                onOpened: entry => page.open(entry)
                onPlayed: entry => page.ask("play", entry.id)
                onSaved: entry => page.ask("save", entry.id)
                onDetailed: entry => Navigation.goKey("sourceEntry", entry.key)
                onClosed: page.quick = null
            }
        }
        footer: Column {
            width: scroll.width; spacing: Metrics.gap24
            Text { visible: page.entries.length === 0 && !page.view.error && !page.indexPage; width: parent.width; wrapMode: Text.Wrap; text: page.view.loading ? I18n.tr("Loading…", I18n.rev) : page.view.inSaved ? page.radio ? I18n.tr("Save stations to find them here.", I18n.rev) : I18n.tr("Subscribe to a show to find it here.", I18n.rev) : I18n.tr("Nothing here matches.", I18n.rev); color: Theme.color.textSecondary; font.pointSize: Type.body }
            PillButton { visible: !page.indexPage && page.view.more === true; enabled: !page.view.loading; text: I18n.tr("Load more", I18n.rev); onClicked: page.ask("more") }
            Item { width: parent.width; height: Metrics.contentBottom }
        }
    }
}
