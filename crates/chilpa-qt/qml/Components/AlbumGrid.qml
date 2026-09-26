import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"

// Every grid of records in the window (frontend ADR-0002 4): a list of
// rows, each laying out a fixed number of cards, so recycling a row only
// rebinds its cards. `columns` follows the width with the design's formula;
// while the width is unknown it is zero and nothing is laid out.
Item {
    id: grid

    property AlbumGridModel model
    // Grow with the rows instead of scrolling, for a shelf inside a page.
    property bool fitContent: false
    // Artists: round portraits, the name centered (design/03 ArtistCard).
    property bool round: false
    // Room below the last row, for what floats over the grid.
    property int bottomInset: 0
    // The artist whose page holds the grid: its menu does not lead there.
    property string artistPage

    readonly property int columns: width <= 0 ? 0
        : Math.max(Metrics.gridMinColumns, Math.floor((width + Metrics.gridGap) / Metrics.cardMinWidth))
    readonly property real cardWidth: columns > 0 ? (width - (columns - 1) * Metrics.gridGap) / columns : 0
    readonly property real rowHeight: cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight

    signal open(int albumId, string title)

    // A click selects while anything is selected or with a modifier held;
    // otherwise it opens the page or the quick view, as Settings says.
    function clicked(at, albumId, title, modifiers) {
        if (!grid.round && (modifiers !== 0 || AlbumActions.selectedCount > 0))
            grid.model.select(at, modifiers === 0 ? 1 : modifiers)
        else if (!grid.round && SettingsModel.quickViewClick)
            toggleQuick(at)
        else
            grid.open(albumId, title)
    }

    function toggleQuick(at) {
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
        spacing: Metrics.gridGap
        reuseItems: true
        cacheBuffer: grid.rowHeight * 2 // derived: two rows ready on each side
        bottomMargin: grid.bottomInset
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: grid.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded }

        delegate: Item {
            id: row

            required property int first
            required property int span

            width: grid.width
            height: grid.rowHeight + (quick.active ? Metrics.gridGap + quick.height : 0)

            Repeater {
                // A fixed count: a recycled row keeps its cards and only
                // rebinds them.
                model: grid.columns

                AlbumCard {
                    required property int index
                    readonly property int at: row.first + index
                    readonly property int rev: grid.model ? grid.model.rev : 0

                    x: index * (grid.cardWidth + Metrics.gridGap)
                    width: grid.cardWidth
                    visible: index < row.span
                    albumId: visible ? grid.model.albumId(at, rev) : 0
                    title: visible ? grid.model.title(at, rev) : ""
                    subtitle: visible ? grid.model.subtitle(at, rev) : ""
                    art: visible ? grid.model.art(at, rev) : ""
                    tint: at
                    round: grid.round
                    selected: !grid.round && AlbumActions.selected(albumId, AlbumActions.rev)
                    selecting: !grid.round && AlbumActions.selectedCount > 0
                    expanded: grid.model.expanded === at
                    onClicked: (modifiers) => grid.clicked(at, albumId, title, modifiers)
                    onQuick: grid.toggleQuick(at)
                    onMenu: (x, y) => {
                        if (grid.round)
                            return
                        const p = mapToItem(grid, x, y)
                        albumMenu.show(albumId, title, subtitle, at, p.x, p.y)
                    }
                }
            }

            Loader {
                id: quick

                y: grid.rowHeight + Metrics.gridGap
                width: grid.width
                active: grid.model.expanded >= row.first && grid.model.expanded < row.first + row.span
                sourceComponent: AlbumQuickView {
                    albumId: grid.model.albumId(grid.model.expanded, grid.model.rev)
                    notchX: (grid.model.expanded - row.first + 0.5) * (grid.cardWidth + Metrics.gridGap) - Metrics.gridGap / 2 // derived: the card's center
                    onClose: grid.model.expanded = -1
                }
            }
        }
    }

    AlbumMenu {
        id: albumMenu

        canExpand: true
        artistPage: grid.artistPage
        onQuick: (at) => grid.toggleQuick(at)
    }
}
