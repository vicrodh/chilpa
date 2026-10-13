import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Column {
    id: shelf
    property string modelKind: "source"
    property var section: ({})
    property string selectedTag: ""
    property bool wrap: false
    // Only records whose title holds every word of it.
    property string filter: ""
    // No sort, no play and no Load more: the page's folded "Other".
    property bool plain: false
    readonly property string activeKey: (section.tags || []).find(t => t.slug === selectedTag)?.key || section.key || ""
    // What the models read by: the shelf's key, or what the document says
    // when the model is the library's (an artist's name).
    readonly property string modelKey: section.model_key || activeKey
    function albumIds() { const ids = []; for (let i = 0; i < cards.albums; i++) ids.push(cards.albumId(i, cards.rev)); return JSON.stringify(ids) }
    spacing: Metrics.gap12
    readonly property bool shown: !!section.error || cards.albums > 0 || tracks.count > 0
    readonly property bool ready: cards.ready && (!tracks.kind || tracks.ready)
    // A page of the rail: the cards that fit across, as QBZ's rails step.
    readonly property int viewportColumns: CardLayout.columns(rail.width)
    readonly property real viewportCardWidth: CardLayout.cardWidth
    readonly property int railCards: Math.max(1, Math.min(section.size || 18, (section.items || []).length || cards.albums))
    readonly property real step: viewportColumns * (viewportCardWidth + Metrics.gridGap) // derived: whole library-sized cards and their gaps
    readonly property real farthest: Math.max(0, rail.contentWidth - rail.width)
    visible: shown
    AlbumGridModel { id: cards; kind: shelf.modelKind; key: shelf.modelKey; limit: shelf.wrap ? 0 : shelf.section.size || 18; columns: grid.columns; order: sorting.sort; group: sorting.grouping; filter: shelf.filter; contentRev: Library.contentRev + Library.remoteRev + Library.remoteArtRev }
    TrackListModel { id: tracks; kind: shelf.section.tracks || shelf.section.tracks_key ? (shelf.section.model || "source") : ""; key: shelf.section.tracks_key || (shelf.section.tracks ? shelf.modelKey : ""); limit: shelf.section.size || Metrics.shelfTrackLimit; filterRev: Library.remoteRev + Library.remoteArtRev; playingPath: Player.path }
    TrackListModel { id: fullTracks; kind: tracks.kind; key: tracks.key; filterRev: Library.remoteRev + Library.remoteArtRev }
    SectionHeader {
        objectName: "shelfHeader"
        width: parent.width; title: shelf.section.title || ""
        sourceIcon: shelf.section.sourceIcon || ""
        link: shelf.wrap || shelf.modelKind !== "source" || !shelf.section.more || !shelf.section.more_key ? "" : I18n.tr("See all", I18n.rev)
        onLinkClicked: shelf.wrap ? Sources.command("section_more", shelf.activeKey, "") : Navigation.goKey("source", shelf.section.more_key || shelf.section.key)
        paged: rail.visible && rail.interactive && shelf.farthest > 0
        canBack: rail.contentX > 1
        canForward: rail.contentX < shelf.farthest - 1
        onBack: rail.contentX = Math.max(0, rail.contentX - shelf.step)
        onForward: rail.contentX = Math.min(shelf.farthest, rail.contentX + shelf.step)
    }
    Text { width: parent.width; visible: !!shelf.section.error; text: shelf.section.error || ""; wrapMode: Text.WordWrap; font.pointSize: Type.body; color: Theme.color.statusNegative }
    Loader {
        width: parent.width
        active: !!shelf.section.header
        sourceComponent: shelf.section.header?.kind === "album" ? albumContext : otherContext
        Component { id: albumContext; SourceContextCard { objectName: "sourceContextCard"; key: shelf.section.header_key || "" } }
        Component { id: otherContext; SourcePageHeader { info: shelf.section.header || ({}) } }
    }
    Flow {
        width: parent.width; spacing: Metrics.gap8; visible: (shelf.section.tags || []).length > 0
        PillButton { text: I18n.tr("All", I18n.rev); onClicked: shelf.selectedTag = "" }
        Repeater { model: shelf.section.tags || []; PillButton { required property var modelData; text: modelData.name; onClicked: shelf.selectedTag = modelData.slug } }
    }
    Item {
        visible: !shelf.plain && (shelf.section.sortable === true || shelf.wrap)
        width: parent.width; height: Math.max(sorting.height, plays.height)
        SourceLibraryControls { id: sorting; anchors.verticalCenter: parent.verticalCenter; source: shelf.activeKey; entity: shelf.section.kind || "album" }
        // Every record of the section, in its order, or one at random: two
        // round buttons at the row's end (the owner, 2026-10-09).
        Row {
            id: plays
            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
            IconButton { objectName: "shelfPlayAll"; visible: shelf.wrap && cards.albums > 0; round: true; size: Metrics.button32; glyph: Icons.playerPlayFilled; filled: true; tip: I18n.tr("Play all", I18n.rev); onClicked: AlbumActions.playMany(shelf.albumIds(), false) }
            IconButton { objectName: "shelfPlayRandom"; visible: shelf.wrap && cards.albums > 1; round: true; size: Metrics.button32; glyph: Icons.arrowsShuffle; tip: I18n.tr("Play random", I18n.rev); onClicked: AlbumActions.play(cards.albumId(Math.floor(Math.random() * cards.albums), cards.rev), false) }
        }
    }
    Flickable {
        id: rail
        width: parent.width; height: grid.implicitHeight + Metrics.gap8
        visible: cards.albums > 0; contentWidth: grid.width; contentHeight: height
        interactive: !shelf.wrap; flickableDirection: Flickable.HorizontalFlick; boundsBehavior: Flickable.StopAtBounds; clip: true
        ScrollBar.horizontal: ViewportScrollBar {}
        AlbumGrid {
            id: grid
            // The rail keeps one card row; its width grows by one card and gap per item.
            width: shelf.wrap || list ? rail.width : Metrics.cardRing * 2 + shelf.railCards * shelf.viewportCardWidth + (shelf.railCards - 1) * Metrics.gridGap // derived: one row of the same cards as the library grid
            height: implicitHeight; fitContent: true
            fixedColumns: shelf.wrap || list ? 0 : shelf.railCards
            viewportX: rail.contentX; viewportWidth: rail.width
            round: shelf.section.round === true; roundIds: shelf.section.artist_cards || []; list: shelf.section.presentation === "ranked"
            model: cards; onOpen: albumId => Navigation.go("album", albumId)
        }
    }
    Row {
        visible: tracks.count > 0; spacing: Metrics.gap8
        PillButton { text: I18n.tr("Play", I18n.rev); onClicked: fullTracks.playAll(false) }
        PillButton { text: I18n.tr("Shuffle", I18n.rev); onClicked: fullTracks.playAll(true) }
        PillButton { text: I18n.tr("Add to queue", I18n.rev); onClicked: fullTracks.queueAll() }
    }
    TrackList { width: parent.width; height: implicitHeight; fitContent: true; visible: tracks.count > 0; model: tracks }
    // More of the shelf comes in under what is there; while it is on its
    // way the button says so and takes no second click.
    PillButton {
        objectName: "shelfLoadMore"
        anchors.horizontalCenter: parent.horizontalCenter
        // Tag shelves have their own keys but no paging cursor. The cursor
        // belongs to the unfiltered shelf, so only that shelf can load it.
        visible: shelf.wrap && !shelf.plain && shelf.selectedTag === "" && (!!shelf.section.more || shelf.section.loading === true)
        enabled: shelf.section.loading !== true
        text: shelf.section.loading === true ? I18n.tr("Loading…", I18n.rev) : I18n.tr("Load more", I18n.rev)
        onClicked: Sources.command("section_more", shelf.activeKey, "")
    }

}
