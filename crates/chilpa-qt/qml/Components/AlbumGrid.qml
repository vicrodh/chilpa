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

    property AlbumGridModel model
    readonly property alias scrollView: view
    readonly property int detailsRev: model ? model.rev : 0
    readonly property bool shelf: !!model && model.maxRows === 1
    // Grow with the rows instead of scrolling, for a shelf inside a page.
    property bool fitContent: false
    property Flickable viewport: null
    readonly property real visibleTop: viewport ? viewport.contentY - Viewport.topIn(grid, viewport.contentItem) : view.contentY
    // How far right of the cards the scrollbar sits: in the middle of the
    // page's margin.
    property real scrollGutter: Metrics.pageGutter
    // Artists: round portraits, the name centered (design/03 ArtistCard).
    property bool round: false
    property var roundIds: []
    function isRound(albumId) { return round || roundIds.includes(albumId) }
    // The sources a card wears: a source's record its own, the library's
    // the logo where sources mix in, and any copy grouped with it its
    // source too.
    // Room below the last row, for what floats over the grid.
    property int bottomInset: 0
    // The artist whose page holds the grid: its menu does not lead there.
    property string artistPage
    // Records as rows, one under another, instead of cards side by side.
    property bool list: false
    // A compact record table with stored technical facts and no cover objects.
    property bool technical: false
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

    Tools.AlbumGridInteraction { id: interaction; grid: grid }

    implicitHeight: shelf ? Math.max(rowHeight, view.contentItem.childrenRect.height) + Metrics.cardRing * 2 : view.contentHeight // derived: the shelf keeps both selection rings

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
        model: grid.model
        leftMargin: grid.shelf ? Metrics.cardRing : 0
        rightMargin: grid.shelf ? Metrics.cardRing : 0
        spacing: grid.list ? Metrics.gap2 : Metrics.gridGap
        reuseItems: true
        cacheBuffer: grid.rowHeight * 2 // derived: two rows ready on each side
        topMargin: Metrics.cardRing
        bottomMargin: grid.bottomInset + Metrics.cardRing
        boundsBehavior: Flickable.StopAtBounds
        header: grid.technical ? technicalHeader : null
        headerPositioning: ListView.OverlayHeader
        ScrollBar.vertical: ViewportScrollBar {
            objectName: "albumVerticalScrollBar"
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
            value: Math.max(view.width, Metrics.nerdyTableWidth)
            restoreMode: Binding.RestoreBindingOrValue
        }

        delegate: Item {
            id: row

            required property int first
            required property int span
            required property int revision
            // The group this row opens, if it opens one.
            required property string group
            readonly property real lead: group !== "" ? Metrics.rowGroup : 0
            readonly property bool nearby: !grid.viewport || y + height >= grid.visibleTop - grid.viewport.height && y < grid.visibleTop + grid.viewport.height * 2

            width: grid.shelf ? grid.cardWidth : grid.technical ? view.contentWidth : grid.width
            height: lead + grid.rowHeight + (quick.active ? Metrics.gap8 + quick.height : 0)

            Text {
                visible: row.group !== ""
                x: Metrics.cardRing
                width: grid.layoutWidth
                height: Metrics.rowGroup
                verticalAlignment: Text.AlignVCenter
                text: row.group
                elide: Text.ElideRight
                font.pointSize: Type.meta
                font.weight: Type.semibold
                color: Theme.color.textSecondary
            }

            AlbumRow {
                readonly property bool shown: grid.list && row.nearby

                visible: shown
                x: Metrics.cardRing
                y: row.lead
                width: grid.layoutWidth
                technical: grid.technical
                rev: grid.detailsRev
                albumId: shown ? grid.model.albumId(row.first, row.revision) : 0
                title: shown ? grid.model.title(row.first, row.revision) : ""
                subtitle: shown ? grid.model.subtitle(row.first, row.revision) : ""
                year: shown ? grid.model.year(row.first, row.revision) : ""
                art: shown && !grid.technical ? grid.model.art(row.first, row.revision) : ""
                columns: grid.listColumns
                sources: shown && !grid.technical && grid.listColumns.indexOf("source") >= 0 ? interaction.badges(albumId, rev) : []
                mixed: !!grid.model && grid.model.mixed
                tint: row.first
                selected: shown && AlbumActions.selected(albumId, AlbumActions.rev)
                expanded: shown && grid.model.expanded === row.first
                available: !shown || AlbumActions.available(albumId, rev)
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

            Repeater {
                // A fixed count: a recycled row keeps its cards and only
                // rebinds them.
                model: grid.list || !row.nearby ? 0 : grid.shelf ? 1 : grid.columns

                AlbumCard {
                    required property int index
                    readonly property bool occupied: index < row.span
                    readonly property int at: row.first + index
                    readonly property int rev: row.revision

                    x: Metrics.cardRing + index * (grid.cardWidth + Metrics.gridGap)
                    y: row.lead
                    width: grid.cardWidth
                    visible: occupied
                    albumId: occupied ? grid.model.albumId(at, rev) : 0
                    sources: occupied ? interaction.badges(albumId, grid.detailsRev) : []
                    title: occupied ? grid.model.title(at, rev) : ""
                    subtitle: occupied ? grid.model.subtitle(at, rev) : ""
                    year: occupied ? grid.model.year(at, rev) : ""
                    art: occupied ? grid.model.art(at, rev) : ""
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

            Loader {
                id: quick
                objectName: "albumQuickLoader"

                y: row.lead + grid.rowHeight + Metrics.gap8
                x: (grid.shelf ? view.contentX - row.x : grid.viewportX) + Metrics.cardRing
                width: Math.max(0, grid.viewportWidth - Metrics.cardRing * 2) // derived: the visible content area
                active: grid.model.expanded >= row.first && grid.model.expanded < row.first + row.span
                sourceComponent: AlbumQuickView {
                    albumId: grid.model.albumId(grid.model.expanded, grid.model.rev)
                    notchX: grid.list ? Metrics.button32 / 2 // derived: under the row's arrow
                          : Math.max(Metrics.gap16, Math.min(width - Metrics.gap16, (grid.model.expanded - row.first + 0.5) * (grid.cardWidth + Metrics.gridGap) - Metrics.gridGap / 2 - grid.viewportX)) // derived: the visible card center
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
