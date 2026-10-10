import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Every grid of records in the window (frontend ADR-0002 4): a list of
// rows, each laying out a fixed number of cards, so recycling a row only
// rebinds its cards. `columns` follows the width with the design's formula;
// while the width is unknown it is zero and nothing is laid out.
Item {
    id: grid

    property AlbumGridModel model
    // When the model last grew at the end: the cards that came then fade in.
    property real freshAt: 0
    Connections { target: grid.model; function onFreshChanged() { if (grid.model.fresh >= 0) grid.freshAt = Date.now() } }
    // Grow with the rows instead of scrolling, for a shelf inside a page.
    property bool fitContent: false
    // Artists: round portraits, the name centered (design/03 ArtistCard).
    property bool round: false
    // Room below the last row, for what floats over the grid.
    property int bottomInset: 0
    // The artist whose page holds the grid: its menu does not lead there.
    property string artistPage
    // Records as rows, one under another, instead of cards side by side.
    property bool list: false
    property real viewportX: 0
    property real viewportWidth: width

    // A selected cover paints outside its card. Keep that ring inside the
    // scrollable plane on every edge instead of clipping boundary cards.
    readonly property real layoutWidth: Math.max(0, width - Metrics.cardRing * 2) // derived: a ring on both sides
    readonly property int columns: layoutWidth <= 0 ? 0 : list ? 1
        : Math.max(Metrics.gridMinColumns, Math.floor((layoutWidth + Metrics.gridGap) / Metrics.cardMinWidth))
    readonly property real cardWidth: columns > 0 ? (layoutWidth - (columns - 1) * Metrics.gridGap) / columns : 0
    readonly property real rowHeight: list ? Metrics.rowRich : cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight

    signal open(int albumId, string title)

    // A click selects while anything is selected or with a modifier held;
    // otherwise it opens the page or the quick view, as Settings says.
    function clicked(at, albumId, title, modifiers, available) {
        if (!available)
            AlbumActions.explain(albumId)
        else if (!grid.round && (modifiers !== 0 || AlbumActions.selectedCount > 0))
            grid.model.select(at, modifiers === 0 ? 1 : modifiers)
        else if (!grid.round && !grid.list && SettingsModel.quickViewClick)
            toggleQuick(at)
        else {
            grid.model.prepareOpen(at)
            grid.open(albumId, title)
        }
    }

    function toggleQuick(at) {
        grid.model.prepareOpen(at)
        grid.model.expanded = grid.model.expanded === at ? -1 : at
    }

    implicitHeight: view.contentHeight

    ListView {
        id: view

        anchors.fill: parent
        interactive: !grid.fitContent
        // Scrolled rows stay inside the grid, not over what sits above it.
        clip: !grid.fitContent
        model: grid.model
        spacing: grid.list ? Metrics.gap2 : Metrics.gridGap
        reuseItems: true
        cacheBuffer: grid.rowHeight * 2 // derived: two rows ready on each side
        topMargin: Metrics.cardRing
        bottomMargin: grid.bottomInset + Metrics.cardRing
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { policy: grid.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded }

        delegate: Item {
            id: row

            required property int first
            required property int span
            // The group this row opens, if it opens one.
            required property string group
            readonly property real lead: group !== "" ? Metrics.rowGroup : 0

            width: grid.width
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
                readonly property int rev: grid.model ? grid.model.rev : 0

                visible: grid.list
                x: Metrics.cardRing
                y: row.lead
                width: grid.layoutWidth
                albumId: visible ? grid.model.albumId(row.first, rev) : 0
                sourceInfo: albumId < 0 ? (JSON.parse(AlbumActions.sources(albumId, rev) || "[]")[0] || {}) : ({})
                title: visible ? grid.model.title(row.first, rev) : ""
                subtitle: visible ? grid.model.subtitle(row.first, rev) : ""
                year: visible ? grid.model.year(row.first, rev) : ""
                art: visible ? grid.model.art(row.first, rev) : ""
                tint: row.first
                selected: visible && AlbumActions.selected(albumId, AlbumActions.rev)
                expanded: visible && grid.model.expanded === row.first
                available: !visible || AlbumActions.available(albumId, rev)
                onClicked: (modifiers) => grid.clicked(row.first, albumId, title, modifiers, available)
                onAvailableChanged: if (!available && expanded) grid.model.expanded = -1
                onQuick: grid.toggleQuick(row.first)
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
                model: grid.list ? 0 : grid.columns

                AlbumCard {
                    required property int index
                    readonly property int at: row.first + index
                    readonly property int rev: grid.model ? grid.model.rev : 0

                    x: Metrics.cardRing + index * (grid.cardWidth + Metrics.gridGap)
                    y: row.lead
                    width: grid.cardWidth
                    visible: index < row.span
                    albumId: visible ? grid.model.albumId(at, rev) : 0
                    sourceInfo: albumId < 0 ? (JSON.parse(AlbumActions.sources(albumId, rev) || "[]")[0] || {}) : ({})
                    title: visible ? grid.model.title(at, rev) : ""
                    subtitle: visible ? grid.model.subtitle(at, rev) : ""
                    year: visible ? grid.model.year(at, rev) : ""
                    art: visible ? grid.model.art(at, rev) : ""
                    artColors: visible && albumId < 0 && !art ? JSON.parse(Sources.artColors(albumId, rev) || "[]") : []
                    qualityInfo: visible && !grid.round ? JSON.parse(AlbumActions.quality(albumId, rev) || "{}") : ({})
                    tint: at
                    round: grid.round
                    fresh: grid.model && grid.model.fresh >= 0 && at >= grid.model.fresh
                    freshAt: grid.freshAt
                    selected: !grid.round && AlbumActions.selected(albumId, AlbumActions.rev)
                    selecting: !grid.round && AlbumActions.selectedCount > 0
                    expanded: grid.model.expanded === at
                    available: grid.round || !visible || AlbumActions.available(albumId, rev)
                    onClicked: (modifiers) => grid.clicked(at, albumId, title, modifiers, available)
                    // A record that stops playing closes its quick view too.
                    onAvailableChanged: if (!available && expanded) grid.model.expanded = -1
                    onQuick: grid.toggleQuick(at)
                    onMenu: (x, y) => {
                        if (grid.round || !available)
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
                x: grid.viewportX + Metrics.cardRing
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

    AlbumMenu {
        id: albumMenu
        objectName: "albumMenu"

        canExpand: true
        artistPage: grid.artistPage
        onQuick: (at) => grid.toggleQuick(at)
    }
}
