import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"
import "../Patterns"

// An artist's page, one for every source and for the library (ui/adr:
// artist page): the header, a jump bar that sticks, then the blocks in
// QBZ's order -- Popular Tracks, Latest release, the sections of releases,
// Appears On, Playlists, Other folded -- and the Network sidebar on the
// right. The facade gives each shelf its role; what has no role is drawn
// last as a plain shelf.
Item {
    id: page
    // "<plugin> native:artist:<id>", or "local artist:<name>".
    property string key
    readonly property string document: Sources.page(key, Library.remoteRev) || "{}"
    readonly property var about: JSON.parse(document)
    readonly property var header: about.header || ({})
    readonly property bool local: about.local === true
    readonly property string name: header.title || ""
    readonly property var network: name !== "" ? JSON.parse(Sources.network(name, Library.remoteRev) || "{}") : ({})
    readonly property var shelves: (about.shelves || []).filter(s => s.sidebar !== true)
    // Open unless the window is narrow with a panel on the right, as QBZ.
    readonly property bool constrained: Window.width < Metrics.networkNarrow && SettingsModel.lyricsOpen
    property bool networkOpen: !constrained
    onConstrainedChanged: if (constrained) networkOpen = false
    onKeyChanged: networkOpen = !constrained
    // The shelves by key, changed only when a shelf comes or goes, so a
    // page read again keeps every block where it was.
    property var shelfKeys: []
    onShelvesChanged: {
        const keys = shelves.map(shelf => shelf.key || shelf.id)
        if (keys.join("\n") !== shelfKeys.join("\n"))
            shelfKeys = keys
    }
    Component.onCompleted: shelvesChanged()
    function shelf(key) { return shelves.find(s => (s.key || s.id) === key) || ({}) }
    function blockOf(id) {
        for (let i = 0; i < blocks.count; i++) {
            const item = blocks.itemAt(i)
            if (item && (item.section.key || item.section.id) === id) return item
        }
        return null
    }
    function jump(id) {
        if (id === "about") { flick.contentY = 0; return }
        const block = blockOf(id)
        if (block) flick.contentY = Math.min(column.y + block.y - Metrics.jumpOffset, Math.max(0, flick.contentHeight - flick.height))
    }
    // The last block the page scrolled past is the one the bar marks.
    readonly property string activeJump: {
        let active = (about.jump || []).length > 0 && about.jump[0].id === "about" ? "about" : ""
        for (let i = 0; i < blocks.count; i++) {
            const item = blocks.itemAt(i)
            if (item && item.visible && column.y + item.y - Metrics.jumpOffset - Metrics.gap2 <= flick.contentY) active = item.section.key || item.section.id
        }
        return active
    }

    Flickable {
        id: flick
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom
        contentWidth: width; contentHeight: column.height + Metrics.contentBottom
        boundsBehavior: Flickable.StopAtBounds; clip: true
        ScrollBar.vertical: ViewportScrollBar {
            overlayParent: page
            z: 2
            viewportHovered: pageHover.hovered
        }
        HoverHandler { id: pageHover; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad }
        Column {
            id: column
            x: Metrics.contentX; y: Metrics.contentTop
            width: flick.width - Metrics.contentX * 2 // derived: padding on both sides
            spacing: Metrics.blockGap
            ArtistHeader { id: head; width: parent.width; info: page.header; local: page.local; networkOpen: page.networkOpen; onToggleNetwork: page.networkOpen = !page.networkOpen }
            // Where the jump bar sits when the page is at the top.
            Item { id: jumpSlot; width: parent.width; height: Metrics.jumpBar }
            Repeater {
                id: blocks
                model: page.shelfKeys
                Loader {
                    id: block
                    required property string modelData
                    readonly property var section: page.shelf(modelData)
                    readonly property string role: section.role || "shelf"
                    width: column.width - panel.width
                    // Each block says whether it has anything to show; its
                    // own visibility would read back the Loader's.
                    visible: item ? item.shown === true : false
                    height: visible ? item.implicitHeight : 0
                    sourceComponent: role === "popular" || (role === "appears" && section.kind === "track") ? trackBlock
                        : role === "latest" ? latestBlock : role === "other" ? otherBlock : shelfBlock
                    onLoaded: item.section = Qt.binding(() => block.section)
                }
            }
            SourceState { width: parent.width; visible: page.shelves.length === 0 && !page.local; source: page.key; about: page.about }
        }
    }
    Component { id: trackBlock; ArtistTracks { actions: (section.role || "") === "popular" } }
    Component {
        id: latestBlock
        Column {
            id: latest
            property var section
            readonly property bool shown: one.albums > 0
            spacing: Metrics.gap12
            visible: shown
            AlbumGridModel { id: one; kind: latest.section.model || "source"; key: latest.section.model_key || latest.section.key || ""; limit: 1; columns: 1; contentRev: Library.remoteRev + Library.remoteArtRev }
            Text { text: latest.section.title || ""; textFormat: Text.PlainText; font.pointSize: Type.section; font.weight: Type.heading; font.letterSpacing: Type.sectionTracking; color: Theme.color.textPrimary }
            AlbumCard {
                width: Metrics.latestCard
                albumId: one.albumId(0, one.rev); title: one.title(0, one.rev); subtitle: one.year(0, one.rev); art: one.art(0, one.rev)
                sourceInfo: albumId < 0 ? (JSON.parse(AlbumActions.sources(albumId, one.rev) || "[]")[0] || {}) : ({})
                qualityInfo: JSON.parse(AlbumActions.quality(albumId, one.rev) || "{}")
                canExpand: false
                onClicked: Navigation.go("album", albumId)
            }
        }
    }
    Component {
        id: shelfBlock
        SourceShelf {
            modelKind: section.model || "source"
            wrap: (section.role || "") === "release" || ((section.role || "") === "appears" && section.kind !== "track")
            filter: page.search
        }
    }
    // Folded by default; "Show" opens the grid, with no sort, no play and
    // no Load more.
    Component {
        id: otherBlock
        Column {
            id: other
            property var section
            property bool expanded: false
            readonly property bool shown: true
            spacing: Metrics.gap12
            Item {
                width: parent.width; height: Metrics.shelfHeader
                Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: other.section.title || ""; textFormat: Text.PlainText; font.pointSize: Type.section; font.weight: Type.heading; font.letterSpacing: Type.sectionTracking; color: Theme.color.textPrimary }
                Text {
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: other.expanded ? I18n.tr("Hide", I18n.rev) : I18n.tr("Show", I18n.rev); font.pointSize: Type.meta; color: Theme.color.accent
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: other.expanded = !other.expanded }
                }
            }
            SourceShelf { visible: other.expanded; width: parent.width; section: other.section; modelKind: other.section.model || "source"; wrap: true; plain: true }
        }
    }
    JumpBar {
        id: bar
        x: flick.x; width: flick.width
        y: Math.max(column.y + jumpSlot.y - flick.contentY, 0)
        pinned: y <= 0
        entries: page.about.jump || []
        active: page.activeJump
        onJump: (id) => page.jump(id)
    }
    readonly property string search: bar.search
    // Under the jump bar, parked at its height once the bar sticks: a card
    // with the side panels' corners and margin, ending a gap above the
    // player bar instead of behind it (the owner, 2026-10-08).
    ArtistNetwork {
        id: panel
        anchors.right: parent.right
        anchors.rightMargin: Metrics.spaceWindow
        y: bar.y + bar.height + Metrics.spaceWindow; height: page.height - y - Metrics.playerSpace - Metrics.spaceWindow * 2 // derived: the bar as tall as it stands, its gap below and the gap between
        Behavior on height { NumberAnimation { duration: Motion.panel; easing.type: Easing.OutCubic } }
        open: page.networkOpen; document: page.about; network: page.network
        onClosed: page.networkOpen = false
    }
}
