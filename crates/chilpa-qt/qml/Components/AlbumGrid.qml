import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Tools
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/Viewport.js" as Viewport
// Every grid of records in the window (frontend ADR-0002 4): a list of
// rows, each laying out a fixed number of cards, so recycling a row only
// rebinds its cards. `columns` follows the width with the design's formula;
// while the width is unknown it is zero and nothing is laid out.
Item {
    id: grid
    property var model
    readonly property alias scrollView: view
    property alias header: view.header
    property alias footer: view.footer
    property alias topMargin: view.topMargin
    property alias bottomMargin: view.bottomMargin
    // A bounded window can provide a count without allocating a row model.
    property int windowCount: -1
    property Component cardDelegate: null
    property int detailsRev: 0
    readonly property int modelRev: model ? model.rev : 0
    onModelRevChanged: if (visible) detailsRev = modelRev
    onVisibleChanged: if (visible) detailsRev = modelRev
    readonly property bool shelf: !!model && model.maxRows === 1
    // Grow with the rows instead of scrolling, for a shelf inside a page.
    property bool fitContent: false
    property Flickable viewport: null
    readonly property real visibleTop: viewport ? viewport.contentY - Viewport.topIn(grid, viewport.contentItem) : view.contentY
    // How far right of the cards the scrollbar sits: in the middle of the
    // page's margin.
    property real scrollGutter: Metrics.pageGutter
    property Item scrollbarOverlay: null // the page whose margin holds the thumb: hover reaches it there
    // Artists: round portraits, the name centered (design/03 ArtistCard).
    property bool round: false
    property var roundIds: []
    function isRound(albumId) { return round || roundIds.includes(albumId) }
    // Room below the last row, for what floats over the grid.
    property int bottomInset: 0
    // The artist whose page holds the grid: its menu does not lead there.
    property string artistPage
    // Records as rows, one under another, instead of cards side by side.
    property bool list: false
    // A compact record table with stored technical facts and no cover objects.
    property bool technical: false
    property var technicalColumns: null
    // The columns of those rows (`AlbumRow.columns`).
    property var listColumns: ["year"]
    property real viewportX: 0
    property real viewportWidth: width
    // A horizontal shelf fixes its one-row column count while keeping the
    // same card width the library grid gives this viewport.
    property int fixedColumns: 0

    // A selected cover paints outside its card. Keep that ring inside the
    // scrollable plane on every edge instead of clipping boundary cards.
    readonly property real layoutWidth: Math.max(0, (technical ? view.contentWidth : width) - Metrics.cardRing * 2) // derived: a ring on both sides
    readonly property int columns: layoutWidth <= 0 ? 0 : list ? 1 : fixedColumns > 0 ? fixedColumns
        : CardLayout.columns(width)
    readonly property real cardWidth: columns > 0 ? CardLayout.cardWidth : 0
    readonly property real rowHeight: technical ? Metrics.rowCompact : list ? Metrics.rowRich : cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight

    signal open(int albumId, string title)

    Tools.AlbumGridInteraction { id: interaction; objectName: "albumGridInteraction"; grid: grid }

    // Measure after delegate layout: a shelf's height also participates in its
    // rows' viewport and quick-view geometry, so it cannot read them in a binding.
    property real measuredShelfHeight: 0
    readonly property var shelfShape: shelf ? [rowHeight, view.contentItem.children] : []
    onShelfShapeChanged: if (shelf) shelfSizing.restart()
    Timer { id: shelfSizing; interval: 0; onTriggered: grid.measuredShelfHeight = interaction.shelfHeight(view, grid.rowHeight) }
    implicitHeight: shelf ? Math.max(rowHeight, measuredShelfHeight) + Metrics.cardRing * 2 : view.contentHeight // derived: room for both selection rings

    Tools.MotionListView {
        id: view
        motionViewport: grid.viewport

        anchors.fill: parent
        anchors.rightMargin: grid.technical ? Metrics.gap16 : 0
        anchors.bottomMargin: grid.technical ? Metrics.gap16 : 0
        flickableDirection: grid.technical ? Flickable.HorizontalAndVerticalFlick : grid.shelf ? Flickable.HorizontalFlick : Flickable.VerticalFlick
        orientation: grid.shelf ? ListView.Horizontal : ListView.Vertical
        interactive: grid.shelf ? contentWidth > width : !grid.fitContent
        // Scrolled rows stay inside the grid, not over what sits above it.
        clip: grid.shelf || !grid.fitContent
        model: grid.windowCount >= 0 ? Math.ceil(grid.windowCount / Math.max(1, grid.columns)) : grid.model
        leftMargin: grid.shelf ? Metrics.cardRing : 0
        rightMargin: grid.shelf ? Metrics.cardRing : 0
        spacing: grid.list ? Metrics.gap2 : Metrics.gridGap
        reuseItems: true
        cacheBuffer: grid.rowHeight * 2 // derived: two rows ready on each side
        topMargin: Metrics.cardRing
        bottomMargin: grid.bottomInset + Metrics.cardRing
        boundsBehavior: Flickable.StopAtBounds
        header: grid.technical ? technicalHeader : null
        headerPositioning: grid.technical ? ListView.OverlayHeader : ListView.InlineHeader
        ScrollBar.vertical: ViewportScrollBar {
            objectName: "albumVerticalScrollBar"; overlayParent: grid.scrollbarOverlay
            policy: grid.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded
            gutter: grid.technical ? Metrics.gap8 : grid.scrollGutter
            viewportHovered: viewHover.hovered
        }
        ScrollBar.horizontal: ViewportScrollBar {
            objectName: "albumHorizontalScrollBar"
            policy: grid.technical ? ScrollBar.AlwaysOn : grid.shelf ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
            gutter: grid.technical ? Metrics.gap8 : -Metrics.gap4
        }
        HoverHandler { id: viewHover; acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad }
        Binding {
            target: view; property: "contentWidth"
            when: grid.technical
            value: Math.max(view.width, view.headerItem ? view.headerItem.minimumWidth : Metrics.nerdyTableWidth)
            restoreMode: Binding.RestoreBindingOrValue
        }

        delegate: Tools.AlbumGridRow {
            id: row
            required property int index
            required property var model
            owner: grid
            onHeightChanged: if (grid.shelf) shelfSizing.restart()
            first: grid.windowCount >= 0 ? index * grid.columns : model.first
            span: grid.windowCount >= 0 ? Math.min(grid.columns, grid.windowCount - first) : model.span
            revision: grid.windowCount >= 0 ? grid.model.rev : model.revision
            group: grid.windowCount >= 0 ? "" : model.group
            listDelegate: Component {
                AlbumRow {
                    width: parent.width
                    technical: grid.technical
                    technicalColumns: grid.technicalColumns
                    rev: grid.detailsRev
                    albumId: grid.model.albumId(row.first, row.cardRevision)
                    title: grid.model.title(row.first, row.cardRevision)
                    explicitContent: grid.model.explicitContent(row.first, row.cardRevision)
                    aiGenerated: grid.model.aiGenerated(row.first, row.cardRevision)
                    subtitle: grid.model.subtitle(row.first, row.cardRevision)
                    year: grid.model.year(row.first, row.cardRevision)
                    art: !grid.technical ? grid.model.art(row.first, row.cardRevision, row.onScreen) : ""
                    columns: grid.listColumns
                    sources: !grid.technical && grid.listColumns.indexOf("source") >= 0 ? interaction.badges(albumId, rev) : []
                    mixed: !!grid.model && grid.model.mixed
                    tint: row.first
                    selected: AlbumActions.selected(albumId, AlbumActions.rev)
                    expanded: grid.model.expanded === row.first
                    available: AlbumActions.available(albumId, rev)
                    onClicked: (modifiers) => interaction.clicked(row.first, albumId, title, modifiers, available)
                    onAvailableChanged: if (!available && expanded) grid.model.expanded = -1
                    onQuick: interaction.toggleQuick(row.first)
                    onMenu: (x, y) => {
                        if (!available)
                            return
                        const p = mapToItem(grid, x, y)
                        albumMenu.show(albumId, title, subtitle, row.first, p.x, p.y)
                    }
                }
            }
            cardDelegate: grid.cardDelegate || standardCard
            Component {
                id: standardCard
                AlbumCard {
                    required property int index
                    readonly property bool occupied: index < row.span
                    readonly property int at: row.first + index
                    readonly property int rev: row.cardRevision
                    x: Metrics.cardRing + index * (grid.cardWidth + Metrics.gridGap)
                    y: row.lead
                    width: grid.cardWidth
                    visible: occupied
                    albumId: occupied ? grid.model.albumId(at, rev) : 0
                    // Show the sources of this record and its grouped copies.
                    sources: occupied ? interaction.badges(albumId, grid.detailsRev) : []
                    title: occupied ? grid.model.title(at, rev) : ""
                    explicitContent: occupied && grid.model.explicitContent(at, rev)
                    aiGenerated: occupied && grid.model.aiGenerated(at, rev)
                    subtitle: occupied ? grid.model.subtitle(at, rev) : ""
                    year: occupied ? grid.model.year(at, rev) : ""
                    art: occupied ? grid.model.art(at, rev, row.onScreen) : ""
                    artColors: occupied && albumId < 0 && String(art) === "" ? JSON.parse(Sources.artColors(albumId, grid.detailsRev) || "[]") : []
                    qualityInfo: occupied && !round ? JSON.parse(AlbumActions.quality(albumId, grid.detailsRev) || "{}") : ({})
                    tint: at
                    round: grid.isRound(albumId)
                    selected: !round && AlbumActions.selected(albumId, AlbumActions.rev)
                    selecting: !round && AlbumActions.selectedCount > 0
                    expanded: grid.model.expanded === at
                    available: round || !occupied || AlbumActions.available(albumId, grid.detailsRev)
                    onClicked: (modifiers) => interaction.clicked(at, albumId, title, modifiers, available)
                    // A record that stops playing closes its quick view too.
                    onAvailableChanged: if (!available && expanded) grid.model.expanded = -1
                    onQuick: interaction.toggleQuick(at)
                    onMenu: (x, y) => {
                        if (round || !available)
                            return
                        const p = mapToItem(grid, x, y)
                        albumMenu.show(albumId, title, subtitle, at, p.x, p.y)
                    }
                }
            }
            quickDelegate: Component {
                AlbumQuickView {
                    viewport: grid.viewport || (grid.fitContent ? null : view)
                    albumId: grid.model.albumId(grid.model.expanded, grid.model.rev)
                    version: parent?.suspended?.album === albumId && parent.suspended.version ? parent.suspended.version : albumId
                    notchX: grid.list ? Metrics.button32 / 2
                          : Math.max(Metrics.gap16, Math.min(width - Metrics.gap16, (grid.model.expanded - row.first + 0.5) * (grid.cardWidth + Metrics.gridGap) - Metrics.gridGap / 2 - grid.viewportX))
                    onClose: grid.model.expanded = -1
                }
            }
        }
    }

    Component {
        id: technicalHeader
        Tools.TechnicalCells {
            width: grid.layoutWidth
            height: Metrics.rowHeader
            header: true
            shown: grid.technicalColumns
        }
    }

    AlbumMenu {
        id: albumMenu
        objectName: "albumMenu"

        canExpand: true
        artistPage: grid.artistPage
        onQuick: (at) => interaction.toggleQuick(at)
    }
}
