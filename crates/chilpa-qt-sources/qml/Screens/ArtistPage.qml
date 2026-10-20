import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
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
    readonly property alias scrollView: flick
    property bool presented: false
    Connections { target: page.Window.window; function onFrameSwapped() { if (page.visible && page.parent?.visible) page.presented = true } }
    // What the listener keeps of the artist in its source, in place of the
    // source's catalog; its key comes from the catalog page's header.
    property bool inLibrary: false
    property string libraryKey
    readonly property string document: Sources.page(inLibrary && libraryKey !== "" ? libraryKey : key, Library.remoteRev) || "{}"
    readonly property var about: JSON.parse(document)
    readonly property var header: about.header || ({title:key.includes("matching-artist-") ? key.slice(key.indexOf(":") + 1) : ""})
    readonly property bool local: about.local === true
    readonly property string name: header.title || ""
    readonly property var network: name !== "" ? JSON.parse(Sources.network(name, Library.remoteRev) || "{}") : ({})
    readonly property var shelves: (about.shelves || []).filter(s => s.sidebar !== true)
    // Open unless the window is narrow with a panel on the right, as QBZ.
    readonly property bool constrained: Window.width < Metrics.networkNarrow && SettingsModel.lyricsOpen
    readonly property bool networkEnabled: network.enabled !== false
    property bool networkOpen: !constrained
    onNetworkEnabledChanged: if (!networkEnabled) networkOpen = false
    onConstrainedChanged: if (constrained) networkOpen = false
    onKeyChanged: { networkOpen = !constrained; inLibrary = false }
    onAboutChanged: if (!inLibrary) { const info = about.header || {}; libraryKey = !local && info.plugin && info.ref ? info.plugin + " artist-library:" + info.ref : "" }
    // The shelves by key, changed only when a shelf comes or goes, so a
    // page read again keeps every block where it was.
    KeyedModel { id: shelfRows; items: page.shelves }
    Component.onCompleted: aboutChanged()
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
            Loader {
                id: head
                objectName: "artistHeaderLoader"
                width: parent.width
                height: Math.max(Metrics.artistPortrait, item ? item.implicitHeight : 0)
                active: page.presented
                asynchronous: true
                sourceComponent: Component { ArtistHeader { info: page.header; local: page.local; networkOpen: page.networkOpen; networkEnabled: page.networkEnabled; sceneAvailable: page.networkEnabled && !!page.network.origin?.place && (page.network.origin?.tags || []).length > 0; onToggleNetwork: page.networkOpen = !page.networkOpen; onSceneRequested: page.askScene(); inLibrary: page.inLibrary; onLibraryChosen: (on) => page.inLibrary = on } }
                ArtTile { id: preview; visible: !head.item; width: Math.min(Metrics.artistPortrait, head.width / 3); height: width; radius: width / 2; source: page.header.art || "" } // derived: one third of the header and a circular crop
                Text { visible: !head.item; x: preview.width + Metrics.blockGap; anchors.verticalCenter: parent.verticalCenter; width: parent.width - x; text: page.header.title || ""; font.pointSize: Type.display; font.weight: Type.bold; color: Theme.color.textPrimary }
            }
            // Where the jump bar sits when the page is at the top.
            Item { id: jumpSlot; width: parent.width; height: Metrics.jumpBar }
            Repeater {
                id: blocks
                model: shelfRows
                Loader {
                    id: block
                    required property var entry
                    readonly property var section: entry
                    readonly property string role: section.role || "shelf"
                    readonly property bool knownEmpty: page.about.loading !== true && ["release", "appears"].includes(role) && (section.items || []).length === 0 && !section.more && section.loading !== true
                    readonly property bool ready: status === Loader.Ready && item?.ready === true
                    property bool mounted: false
                    width: column.width - panel.width
                    // Each block says whether it has anything to show; its
                    // own visibility would read back the Loader's.
                    visible: !knownEmpty && (!ready || item.shown === true)
                    height: visible ? ready ? Math.max(skeleton.implicitHeight, item.implicitHeight) : skeleton.implicitHeight : 0
                    active: !knownEmpty && page.presented && page.about.loading !== true && (mounted || column.y + y < flick.contentY + flick.height * 2)
                    asynchronous: true
                    sourceComponent: role === "popular" || (role === "appears" && section.kind === "track") ? trackBlock
                        : role === "latest" ? latestBlock : role === "other" ? otherBlock : shelfBlock
                    onLoaded: mounted = true
                    Column {
                        id: skeleton
                        width: parent.width; spacing: Metrics.gap12
                        visible: !block.ready && block.visible
                        Text { text: block.section.title || ""; font.pointSize: Type.section; font.weight: Type.heading; color: Theme.color.textPrimary }
                        ShelfSkeleton { width: parent.width; tracks: block.role === "popular"; trackRowHeight: SettingsModel.density === "rich" ? Metrics.rowRich : Metrics.rowCompact; count: tracks ? block.section.preview || Metrics.previewRows : block.role === "latest" ? 1 : CardLayout.columns(width); oneRow: true }
                    }
                }
            }
            SourceState { width: parent.width; visible: page.shelves.length === 0 && !page.local; source: page.key; about: page.about }
        }
    }
    Component { id: trackBlock; ArtistTracks { section: parent.section; viewport: flick; actions: (section.role || "") === "popular" } }
    Component {
        id: latestBlock
        Column {
            id: latest
            property var section: parent.section
            readonly property bool ready: one.ready
            readonly property bool shown: one.albums > 0
            spacing: Metrics.gap12
            visible: shown
            AlbumGridModel { id: one; kind: latest.section.model || "source"; key: latest.section.model_key || latest.section.key || ""; limit: 1; columns: 1; contentRev: Library.remoteRev + Library.remoteArtRev }
            Text { text: latest.section.title || ""; textFormat: Text.PlainText; font.pointSize: Type.section; font.weight: Type.heading; font.letterSpacing: Type.sectionTracking; color: Theme.color.textPrimary }
            AlbumCard {
                width: CardLayout.cardWidth
                albumId: one.albumId(0, one.rev); title: one.title(0, one.rev); subtitle: one.subtitle(0, one.rev); year: one.year(0, one.rev); art: one.art(0, one.rev)
                sources: albumId < 0 ? JSON.parse(AlbumActions.sources(albumId, one.rev) || "[]") : []
                qualityInfo: JSON.parse(AlbumActions.quality(albumId, one.rev) || "{}")
                canExpand: false
                onClicked: Navigation.go("album", albumId)
            }
        }
    }
    Component {
        id: shelfBlock
        SourceShelf {
            section: parent.section
            viewport: flick
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
            property var section: parent.section
            readonly property bool ready: true
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
            Loader { width: parent.width; active: other.expanded; sourceComponent: Component { SourceShelf { viewport: flick; width: parent.width; section: other.section; modelKind: other.section.model || "source"; wrap: true; plain: true } } }
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
    Item {
        id: panel
        width: page.networkEnabled && page.networkOpen ? Metrics.networkPanel : 0
        visible: page.networkEnabled
        Behavior on width { NumberAnimation { duration: Motion.networkSlide; easing.type: Easing.InOutQuad } }
        anchors.right: parent.right
        anchors.rightMargin: Metrics.spaceWindow
        y: bar.y + bar.height + Metrics.spaceWindow; height: page.height - y - Metrics.playerSpace - Metrics.spaceWindow * 2 // derived: the bar as tall as it stands, its gap below and the gap between
        Behavior on height { NumberAnimation { duration: Motion.panel; easing.type: Easing.OutCubic } }
        Loader {
            objectName: "artistNetworkLoader"
            anchors.fill: parent
            active: page.presented && page.networkEnabled
            asynchronous: true
            sourceComponent: Component { ArtistNetwork {
                width: parent.width; height: parent.height
                open: page.networkOpen; document: page.about; network: page.network
                onClosed: page.networkOpen = false
                onSceneRequested: page.askScene()
            } }
        }
    }
    function askScene() { sceneChoice.active = true; sceneChoice.item.open() }
    Loader {
        id: sceneChoice
        active: false
        sourceComponent: Component { Modal {
        objectName: "sceneChoice"
        width: Math.min(Metrics.settingsMaxWidth, page.width - Metrics.modalInsets)
        Column {
            width: parent.width; spacing: Metrics.gap16
            Text { width: parent.width; text: I18n.tr("Artists from this scene", I18n.rev); font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
            Text { width: parent.width; text: I18n.tr("Include matches from your plugins?", I18n.rev); wrapMode: Text.WordWrap; font.pointSize: Type.body; color: Theme.color.textSecondary }
            Row {
                spacing: Metrics.gap8
                PillButton { text: I18n.tr("Without plugins", I18n.rev); onClicked: page.openScene(false) }
                PillButton { objectName: "sceneIncludePlugins"; text: I18n.tr("Include plugins", I18n.rev); kind: "primary"; onClicked: page.openScene(true) }
            }
        }
        } }
    }
    function openScene(includePlugins) {
        sceneChoice.item.close()
        Sources.sceneOpen(page.name, includePlugins)
        Navigation.goKey("artistScene", page.name)
    }
}
