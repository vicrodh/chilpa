import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "../Components"
import "../Patterns"

// The results page, as QBZ's (SearchView): what a search found, whole, by
// tab. All has the top result beside the artists, then records in pages of
// up to three rows, six songs and six more at a time, and playlists; each other
// tab lists its own, twenty more at a time. Records, artists and playlists
// are drawn with the cards they have everywhere else, and every result says
// whose it is: its source's icon, or Chilpa's mark for the library's own.
// The sources and quality are the flyout's, and so is the switch that brings
// the results back under the search box.
Flickable {
    id: page
    // What was typed: the route's key.
    property string query
    property string tab: "all"
    property int shown: Metrics.searchPage
    // How many songs All lists, six until more are asked for.
    property int songsShown: Metrics.searchSongs
    property var rows: []
    // What finds the rows, for whoever drives the page.
    readonly property alias search: results
    readonly property var tabs: [{id: "all", scope: "page", text: I18n.tr("All", I18n.rev)}, {id: "albums", scope: "albums", text: I18n.tr("Albums", I18n.rev)},
        {id: "songs", scope: "songs", text: I18n.tr("Tracks", I18n.rev)}, {id: "artists", scope: "artists", text: I18n.tr("Artists", I18n.rev)},
        {id: "playlists", scope: "playlists", text: I18n.tr("Playlists", I18n.rev)}]
    // The rows by section, each with its place in the model.
    readonly property var sections: {
        const out = {top: [], artists: [], albums: [], songs: [], playlists: []}
        rows.forEach((row, at) => { if (row.kind > 0 && row.kind !== 5 && out[row.section]) out[row.section].push(Object.assign({at: at}, row)) })
        return out
    }
    readonly property var listed: tab === "all" ? [] : sections[tab] || []
    function read() { rows = JSON.parse(results.rowsJson() || "[]") }
    function open(at) { results.act(at, "open"); navigation.flush() }
    function menu(at, item, x, y) { const point = item.mapToItem(column, x, y); hitMenu.show(at, point.x, point.y) }
    function show(id) { tab = id; shown = Metrics.searchPage; songsShown = Metrics.searchSongs; contentY = 0 }
    function artUrl(art) { return !art ? "" : art.startsWith("https://") ? art : "file://" + art.split("/").map(encodeURIComponent).join("/") }
    // Whose a row is: a source names itself, and the library's own are Chilpa's.
    function mine(row) { return (row.sourceName || "") === "" }
    function origin(row) { return mine(row) ? ({chilpa: true}) : ({icon: row.sourceIcon || ""}) }
    function owner(row) { return mine(row) ? "Chilpa" : row.sourceName }
    onQueryChanged: show("all")

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    clip: true
    ScrollBar.vertical: ViewportScrollBar {}

    SearchModel {
        id: results
        text: page.query
        scope: (page.tabs.find(t => t.id === page.tab) || {}).scope || "page"
    }
    Connections { target: results; function onModelReset() { page.read() } }
    Timer { interval: Motion.poll; repeat: true; running: page.visible; onTriggered: results.refresh() }
    SearchNavigation { id: navigation; search: results }

    component Hits: Column {
        id: hits
        property var rows: []
        width: parent ? parent.width : 0
        Repeater {
            model: hits.rows
            Item {
                required property var modelData
                width: hits.width; height: hit.height
                SearchHit {
                    id: hit
                    width: parent.width
                    index: parent.modelData.at; kind: parent.modelData.kind
                    title: parent.modelData.title; detail: parent.modelData.detail
                    art: page.artUrl(parent.modelData.art); duration: parent.modelData.duration
                    sourceIcon: parent.modelData.sourceIcon; sourceName: parent.modelData.sourceName
                    markOwn: true
                    onActivated: page.open(index)
                    onPlayed: { results.act(index, "play"); navigation.flush() }
                    onMenu: (left, top) => page.menu(index, hit, left, top)
                }
            }
        }
    }

    Component {
        id: recordCard
        SearchRecordCard {
            id: record
            required property var modelData
            row: modelData
            onClicked: page.open(modelData.at)
            onMenu: (x, y) => page.menu(modelData.at, record, x, y)
        }
    }
    Component {
        id: artistCard
        SceneArtistCard {
            id: person
            required property var modelData
            artist: ({title: modelData.title, sources: [{local: page.mine(modelData), chilpa: page.mine(modelData), plugin: page.mine(modelData) ? "" : modelData.sourceName,
                icon: modelData.sourceIcon || "", name: page.owner(modelData)}]})
            art: page.artUrl(modelData.art)
            onActivated: page.open(modelData.at)
            onSourceActivated: page.open(modelData.at)
            onMenuAsked: (x, y) => page.menu(modelData.at, person, x, y)
        }
    }
    Component {
        id: playlistCard
        PlaylistCard {
            id: list
            required property var modelData
            name: modelData.title || ""
            art: modelData.art || ""
            origin: page.origin(modelData)
            detail: page.owner(modelData)
            onActivated: page.open(modelData.at)
            onPlayed: { results.act(modelData.at, "play"); navigation.flush() }
            onMenuAsked: (x, y) => page.menu(modelData.at, list, x, y)
        }
    }

    Column {
        id: column
        x: Metrics.contentX; y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        Item {
            width: parent.width; height: Metrics.shelfHeader
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Search", I18n.rev)
                font.pointSize: Type.title; font.weight: Type.bold; font.letterSpacing: Type.titleTracking
                color: Theme.color.textPrimary
            }
            SearchPreviewSwitch { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; search: results }
        }
        SearchFilters { width: parent.width; search: results }
        Row {
            objectName: "searchTabs"
            spacing: Metrics.gap24
            Repeater {
                model: page.tabs
                Column {
                    required property var modelData
                    readonly property bool active: page.tab === modelData.id
                    objectName: "searchTab_" + modelData.id
                    spacing: Metrics.gap4
                    Text {
                        text: parent.modelData.text
                        font.pointSize: Type.body; font.weight: parent.active ? Type.bold : Type.regular
                        color: parent.active ? Theme.color.textPrimary : Theme.color.textSecondary
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: page.show(parent.parent.modelData.id) }
                    }
                    Rectangle { visible: parent.active; width: parent.width; height: Metrics.gap2; radius: height / 2; color: Theme.color.accent } // derived: a rounded underline
                }
            }
        }

        Row {
            visible: page.tab === "all"
            width: parent.width
            spacing: Metrics.gridGap
            Column {
                visible: page.sections.top.length > 0
                spacing: Metrics.gap8
                SectionHeader { width: hero.width; title: I18n.tr("Top result", I18n.rev) }
                SearchResultCard { id: hero; row: page.sections.top[0] || ({}); onActivated: page.open(row.at); onMenuAsked: (x, y) => page.menu(row.at, hero, x, y) }
            }
            SearchRail {
                id: artistsRail
                width: parent.width - (page.sections.top.length > 0 ? hero.width + parent.spacing : 0)
                title: I18n.tr("Artists", I18n.rev)
                rows: page.sections.artists.slice(0, Metrics.searchRail)
                card: artistCard
                cellWidth: Metrics.sceneCardWidth
                cellHeight: Metrics.sceneCardHeight
                onViewAll: page.show("artists")
            }
        }
        SearchRail {
            id: albumsRail
            objectName: "searchAlbumsRail"
            visible: page.tab === "all" && rows.length > 0
            width: parent.width
            title: I18n.tr("Albums", I18n.rev)
            rows: page.sections.albums.slice(0, Metrics.searchRail)
            lines: Metrics.searchRecordRows
            card: recordCard
            onViewAll: page.show("albums")
        }
        Column {
            visible: page.tab === "all" && page.sections.songs.length > 0
            width: parent.width
            spacing: Metrics.gap8
            SectionHeader {
                width: parent.width
                title: I18n.tr("Tracks", I18n.rev)
                link: page.sections.songs.length > Metrics.searchSongs ? I18n.tr("See all", I18n.rev) : ""
                onLinkClicked: page.show("songs")
            }
            Hits { rows: page.sections.songs.slice(0, page.songsShown) }
            PillButton {
                objectName: "searchSongsMore"
                visible: page.sections.songs.length > page.songsShown
                anchors.horizontalCenter: parent.horizontalCenter
                text: I18n.tr("Load more", I18n.rev) + " (" + page.songsShown + " / " + page.sections.songs.length + ")"
                // A few more where they are; "See all" is the tab with every one.
                onClicked: page.songsShown += Metrics.searchSongs
            }
        }
        SearchRail {
            id: playlistsRail
            visible: page.tab === "all" && rows.length > 0
            width: parent.width
            title: I18n.tr("Playlists", I18n.rev)
            rows: page.sections.playlists.slice(0, Metrics.searchRail)
            card: playlistCard
            onViewAll: page.show("playlists")
        }

        Flow {
            visible: page.tab !== "all" && page.tab !== "songs"
            width: parent.width
            spacing: Metrics.gridGap
            Repeater {
                model: page.listed.slice(0, page.shown)
                delegate: page.tab === "artists" ? artistCard : page.tab === "playlists" ? playlistCard : recordCard
            }
        }
        Hits { visible: page.tab === "songs"; rows: page.tab === "songs" ? page.listed.slice(0, page.shown) : [] }
        PillButton {
            objectName: "searchLoadMore"
            visible: page.listed.length > page.shown
            anchors.horizontalCenter: parent.horizontalCenter
            text: I18n.tr("Load more", I18n.rev) + " (" + page.shown + " / " + page.listed.length + ")"
            onClicked: page.shown += Metrics.searchPage
        }
        Text {
            visible: results.count === 0 && page.query.trim() !== ""
            text: I18n.trArgs("Nothing found for “{}”", [page.query], I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        SearchMenu { id: hitMenu; navigation: navigation; search: results }
    }
}
