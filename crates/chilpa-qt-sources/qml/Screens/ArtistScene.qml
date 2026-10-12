import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// Filtering, score order, grouped virtual rows, pinned controls, scroll hero
// and alphabet rail. Catalog matches are optional.
Item {
    id: root
    readonly property var doc: JSON.parse(Sources.scene(Library.remoteRev) || "{}")
    readonly property var allArtists: doc.artists || []
    readonly property bool ready: doc.state === "ready"
    readonly property bool canBrowse: allArtists.some(function (artist) { return (artist.sources || []).length > 0 })
    property string mode: "grid"
    property string searchQuery: ""
    property bool groupingEnabled: false
    property var activeGenres: []
    property bool genrePopupOpen: false
    property string selectedArtistId: ""
    onCanBrowseChanged: if (!canBrowse) { mode = "grid"; selectedArtistId = "" }
    readonly property string sceneTitle: doc.sceneLabel || ""
    readonly property string flagSource: doc.countryCode
        ? "qrc:/qt/qml/app/chilpa/sources/assets/flags/" + String(doc.countryCode).toLowerCase() + ".svg" : ""
    readonly property var filteredArtists: {
        // Apply the AND genre filter, then trimmed text search.
        var out = root.allArtists
        if (root.activeGenres.length > 0) {
            var want = root.activeGenres
            out = out.filter(function (a) {
                var g = a.genres || []
                for (var i = 0; i < want.length; ++i)
                    if (g.indexOf(want[i]) === -1) return false
                return true
            })
        }
        var q = (root.searchQuery || "").trim().toLowerCase()
        if (q !== "") out = out.filter(function (a) { return root.artistNameOf(a).toLowerCase().indexOf(q) !== -1 })
        return out
    }
    readonly property bool filteringActive: searchQuery.trim() !== "" || activeGenres.length > 0
    readonly property var availableGenres: {
        var counts = ({}), order = []
        for (var i = 0; i < allArtists.length; ++i) {
            var g = allArtists[i].genres || []
            for (var j = 0; j < g.length; ++j) {
                if (counts[g[j]] === undefined) { counts[g[j]] = 0; order.push(g[j]) }
                counts[g[j]] += 1
            }
        }
        var rows = order.map(function (n) { return {name:n, count:counts[n]} })
        rows.sort(function (a, b) { return b.count - a.count })
        return rows.map(function (r) { return r.name })
    }
    function artistNameOf(a) { return String((a && (a.title || a.name)) || "") }
    function alphaKeyOf(name) {
        var c = (name || "").charAt(0).toUpperCase()
        return (c >= "A" && c <= "Z") ? c : "#"
    }
    function groupArtists(list) {
        var sorted = list.slice().sort(function (a,b) { return root.artistNameOf(a).localeCompare(root.artistNameOf(b)) })
        var buckets = ({}), keys = []
        for (var i = 0; i < sorted.length; ++i) {
            var k = root.alphaKeyOf(root.artistNameOf(sorted[i]))
            if (buckets[k] === undefined) { buckets[k] = []; keys.push(k) }
            buckets[k].push(sorted[i])
        }
        keys.sort(function (a,b) { if (a === "#") return -1; if (b === "#") return 1; return a.localeCompare(b) })
        return keys.map(function (k) { return {key:k, artists:buckets[k]} })
    }
    readonly property var groups: groupingEnabled ? groupArtists(filteredArtists) : [{key:"", artists:filteredArtists}]
    readonly property var panelArtists: {
        var out = [], grouped = groupArtists(filteredArtists)
        for (var i = 0; i < grouped.length; ++i)
            for (var j = 0; j < grouped[i].artists.length; ++j) out.push(grouped[i].artists[j])
        return out
    }
    readonly property int cardW: Metrics.sceneCardWidth
    readonly property int cardH: Metrics.sceneCardHeight
    readonly property int cardGap: Metrics.sceneCardGap
    readonly property int rowGap: Metrics.sceneRowGap
    readonly property int headerH: Metrics.sceneGroupHeader
    readonly property int columns: Math.max(1, Math.floor((gridPane.width + cardGap) / (cardW + cardGap)))
    readonly property var rowModel: {
        var out = []
        for (var g = 0; g < root.groups.length; ++g) {
            var group = root.groups[g]
            if (root.groupingEnabled && group.key !== "") out.push({type:"header", key:group.key, count:group.artists.length})
            for (var i = 0; i < group.artists.length; i += root.columns)
                out.push({type:"row", key:group.key, artists:group.artists.slice(i, i + root.columns)})
        }
        return out
    }
    readonly property bool heroScrolledPast: mode === "grid" && grid.contentY > 160
    function jumpToLetter(letter) {
        for (var i = 0; i < rowModel.length; ++i) {
            if (rowModel[i].type !== "header" || rowModel[i].key !== letter) continue
            var y = grid.headerItem ? grid.headerItem.height : 0
            for (var j = 0; j < i; ++j) y += rowModel[j].type === "header" ? headerH : cardH + rowGap
            jumpAnimation.stop(); jumpAnimation.from = grid.contentY
            jumpAnimation.to = Math.min(Math.max(0, grid.contentHeight - grid.height), y)
            jumpAnimation.start()
            return
        }
    }
    function openArtist(artist) {
        var first = (artist.sources || [])[0]
        if (first && first.plugin && first.ref) Navigation.goKey("source", first.plugin + " native:" + first.ref)
        else Navigation.openArtist(artist.title || "")
    }
    Component.onDestruction: Sources.sceneClose()

    SceneLoading {
        anchors.fill: parent; visible: doc.state === "loading"
        progress: root.doc.progress || ({})
        includeSources: root.doc.includeSources === true
        onRetry: Sources.sceneRetry()
    }
    Column {
        anchors.centerIn: parent; spacing: Metrics.gap16
        visible: doc.state === "error"
        Text { width: Math.min(Metrics.sceneErrorWidth, root.width - Metrics.sceneErrorInset); text: doc.error || ""; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter; color: Theme.color.textSecondary }
        PillButton { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.tr("Retry", I18n.rev); onClicked: Sources.sceneRetry() }
    }
    Text {
        visible: ready && allArtists.length === 0
        anchors.centerIn: parent; text: I18n.tr("No artists from this scene were found.", I18n.rev)
        font.pointSize: Type.body; color: Theme.color.textSecondary
    }
    Item {
        id: content
        anchors.fill: parent; visible: ready && allArtists.length > 0
        Item {
            id: compact
            x: Metrics.scenePadLeft; width: parent.width - Metrics.scenePadTotal; height: root.heroScrolledPast ? Metrics.sceneCompactHeight : 0
            visible: root.heroScrolledPast
            Row {
                anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap10
                Image { width: Metrics.sceneCompactFlag; height: Metrics.sceneCompactFlag; source: root.flagSource; visible: status === Image.Ready }
                Text { text: root.sceneTitle; font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
                Text { text: root.doc.genreSummary || ""; font.pointSize: Type.meta; color: Theme.color.textTertiary }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.color.strokeHairline }
        }
        Item {
            id: nav
            x: Metrics.scenePadLeft; y: compact.height; width: parent.width - Metrics.scenePadTotal; height: Metrics.sceneNavHeight
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.filteringActive ? I18n.tr("{} / {} artists", I18n.rev).replace("{}", root.filteredArtists.length).replace("{}", root.allArtists.length)
                    : I18n.tr("{} artists", I18n.rev).replace("{}", root.allArtists.length)
                font.pointSize: Type.meta; color: Theme.color.textSecondary
            }
            Row {
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
                SearchField { width: Metrics.sceneSearchWidth; placeholder: I18n.tr("Search…", I18n.rev); onTextChanged: root.searchQuery = text; onEscaped: text = "" }
                PillButton { id: genreButton; visible: root.availableGenres.length > 1; text: root.activeGenres.length > 0 ? I18n.tr("Genres ({})", I18n.rev).replace("{}", root.activeGenres.length) : I18n.tr("Genres", I18n.rev); onClicked: root.genrePopupOpen = !root.genrePopupOpen }
                PillButton { visible: root.canBrowse; text: root.mode === "grid" ? I18n.tr("Browse view", I18n.rev) : I18n.tr("Grid view", I18n.rev); onClicked: { root.mode = root.mode === "grid" ? "sidepanel" : "grid"; root.selectedArtistId = ""; root.genrePopupOpen = false } }
                PillButton { visible: root.mode === "grid"; text: root.groupingEnabled ? I18n.tr("Group: A-Z", I18n.rev) : I18n.tr("Group: Off", I18n.rev); onClicked: root.groupingEnabled = !root.groupingEnabled }
            }
        }
        Item {
            anchors.left: parent.left; anchors.right: parent.right; anchors.top: nav.bottom; anchors.bottom: parent.bottom
            Item {
                id: gridPane
                x: Metrics.scenePadLeft; width: parent.width - Metrics.scenePadTotal - (root.groupingEnabled ? Metrics.scenePadTotal : 0); height: parent.height
                visible: root.mode === "grid"
                ListView {
                    id: grid
                    anchors.fill: parent; clip: true; boundsBehavior: Flickable.StopAtBounds
                    model: root.rowModel; cacheBuffer: (root.cardH + root.rowGap) * 2
                    NumberAnimation { id: jumpAnimation; target: grid; property: "contentY"; duration: Metrics.sceneJumpDuration; easing.type: Easing.OutCubic }
                    header: Item {
                        width: grid.width; height: Metrics.sceneHeroHeight
                        Row {
                            width: parent.width; spacing: Metrics.modalInset
                            Image { id: flag; width: Metrics.sceneHeroFlag; height: Metrics.sceneHeroFlag; source: root.flagSource; visible: status === Image.Ready }
                            Column {
                                anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
                                width: parent.width - (flag.visible ? Metrics.sceneHeroHeight : 0)
                                Text { width: parent.width; text: root.sceneTitle; elide: Text.ElideRight; font.pointSize: Type.title; font.weight: Type.bold; color: Theme.color.textPrimary }
                                Text { width: parent.width; text: I18n.tr("Based on {artist} · {genres}", I18n.rev).replace("{artist}", root.doc.artistName || "").replace("{genres}", root.doc.genreSummary || ""); wrapMode: Text.WordWrap; font.pointSize: Type.body; color: Theme.color.textSecondary }
                            }
                        }
                    }
                    delegate: Item {
                        id: row
                        required property var modelData
                        readonly property bool heading: modelData.type === "header"
                        width: grid.width; height: heading ? root.headerH : root.cardH + root.rowGap
                        Text { visible: row.heading; y: 12; text: (row.modelData.key || "").toUpperCase(); font.pointSize: Type.meta; font.weight: Type.semibold; color: Theme.color.textTertiary }
                        Text { visible: row.heading; anchors.right: parent.right; y: 12; text: row.modelData.count || ""; font.pointSize: Type.meta; color: Theme.color.textTertiary }
                        Row {
                            visible: !row.heading; spacing: root.cardGap
                            Repeater {
                                model: row.heading ? [] : row.modelData.artists || []
                                SceneArtistCard { required property var modelData; artist: modelData; onActivated: root.openArtist(modelData) }
                            }
                        }
                    }
                    footer: Item {
                        width: grid.width; height: Metrics.sceneFooterHeight
                        Column {
                            anchors.horizontalCenter: parent.horizontalCenter; spacing: Metrics.gap12
                            Text { visible: !!root.doc.error; text: root.doc.error || ""; color: Theme.color.statusNegative }
                            PillButton { visible: root.doc.hasMore === true && root.doc.loadingMore !== true; text: I18n.tr("Load more", I18n.rev); onClicked: Sources.sceneMore() }
                            Text { visible: root.doc.loadingMore === true; text: I18n.tr("Loading…", I18n.rev) + " " + (root.doc.progress?.percentage || 0) + "%"; color: Theme.color.textSecondary }
                        }
                    }
                }
                Text { visible: root.filteredArtists.length === 0; anchors.horizontalCenter: parent.horizontalCenter; y: Metrics.sceneNoResultY; text: I18n.tr("No results found.", I18n.rev); color: Theme.color.textSecondary }
            }
            SceneAlphaRail {
                visible: root.mode === "grid" && root.groupingEnabled
                anchors.right: parent.right; anchors.rightMargin: Metrics.gap8; y: 0
                activeKeys: root.groups.map(g => g.key)
                onJumped: (letter) => root.jumpToLetter(letter)
            }
            ScenePanel {
                visible: root.mode === "sidepanel"; anchors.fill: parent
                artists: root.panelArtists
                includeSources: root.doc.includeSources === true
                selectedId: root.selectedArtistId
                onArtistSelected: (id) => root.selectedArtistId = id
                onArtistOpened: (artist) => root.openArtist(artist)
            }
        }
    }
    MouseArea {
        visible: root.genrePopupOpen; z: 19; anchors.fill: parent
        cursorShape: Qt.ArrowCursor
        onClicked: root.genrePopupOpen = false
    }
    SceneGenreFilter {
        visible: root.genrePopupOpen && root.ready
        z: 20; width: Math.min(Metrics.sceneGenreWidth, root.width - Metrics.sceneGenreInset)
        x: Math.max(Metrics.gap8, genreButton.mapToItem(root, 0, 0).x + genreButton.width - width)
        y: genreButton.mapToItem(root, 0, genreButton.height).y + Metrics.gap6
        genres: root.availableGenres; selected: root.activeGenres
        onToggled: (genre) => root.activeGenres = root.activeGenres.includes(genre) ? root.activeGenres.filter(g => g !== genre) : root.activeGenres.concat([genre])
        onCleared: root.activeGenres = []
        onClosed: root.genrePopupOpen = false
    }
}
