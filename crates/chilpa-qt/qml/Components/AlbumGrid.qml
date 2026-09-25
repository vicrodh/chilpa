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
    // Room below the last row, for what floats over the grid.
    property int bottomInset: 0

    readonly property int columns: width <= 0 ? 0
        : Math.max(Metrics.gridMinColumns, Math.floor((width + Metrics.gridGap) / Metrics.cardMinWidth))
    readonly property real cardWidth: columns > 0 ? (width - (columns - 1) * Metrics.gridGap) / columns : 0
    readonly property real rowHeight: cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight

    signal open(int albumId)

    implicitHeight: view.contentHeight

    ListView {
        id: view

        anchors.fill: parent
        interactive: !grid.fitContent
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
            height: grid.rowHeight

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
                    title: visible ? grid.model.title(at, rev) : ""
                    subtitle: visible ? grid.model.subtitle(at, rev) : ""
                    art: visible ? grid.model.art(at, rev) : ""
                    tint: at
                    onOpen: grid.open(grid.model.albumId(at, rev))
                    onPlay: grid.model.play(at)
                }
            }
        }
    }
}
