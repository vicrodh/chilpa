import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A section of the results page as a carousel of cards (QBZ's carousels),
// paged by the header's arrows, with "See all" to the section's own tab. A
// page holds up to `lines` rows of the columns that fit, read across and
// down as the Home's compact grid, and only as many rows as it fills.
Column {
    id: rail
    property string title
    // The section's rows, each with its place in the search model (`at`).
    property var rows: []
    property int lines: 1
    // What draws a row: given `modelData`, and placed by the rail.
    property Component card
    property real cellWidth: CardLayout.cardWidth
    property real cellHeight: CardLayout.cardHeight
    signal viewAll()
    readonly property real pitch: cellWidth + Metrics.gridGap
    readonly property int columns: Math.max(1, Math.floor((width + Metrics.gridGap) / pitch))
    readonly property int used: Math.max(1, Math.min(lines, Math.ceil(rows.length / columns)))
    readonly property int perPage: columns * used
    readonly property int pageCount: Math.max(1, Math.ceil(rows.length / perPage))
    readonly property real step: columns * pitch // derived: a page of cards
    readonly property real farthest: (pageCount - 1) * step
    function cellX(index) { return Math.floor(index / perPage) * step + index % perPage % columns * pitch }
    function cellY(index) { return Math.floor(index % perPage / columns) * (cellHeight + Metrics.gridGap) }
    visible: rows.length > 0
    spacing: Metrics.gap8

    SectionHeader {
        width: parent.width
        title: rail.title
        link: I18n.tr("See all", I18n.rev)
        paged: rail.farthest > 0
        canBack: pages.contentX > 1
        canForward: pages.contentX < rail.farthest - 1
        onBack: pages.contentX = Math.max(0, pages.contentX - rail.step)
        onForward: pages.contentX = Math.min(rail.farthest, pages.contentX + rail.step)
        onLinkClicked: rail.viewAll()
    }
    Flickable {
        id: pages
        objectName: "searchRailPages"
        width: parent.width
        height: rail.used * rail.cellHeight + (rail.used - 1) * Metrics.gridGap + Metrics.gap8 // derived: the rows, the gaps between and room for the bar
        contentWidth: rail.farthest + width
        contentHeight: height
        clip: true
        interactive: rail.pageCount > 1
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ViewportScrollBar {}
        Repeater {
            model: rail.rows
            delegate: rail.card
            onItemAdded: (index, item) => {
                item.x = Qt.binding(() => rail.cellX(index))
                item.y = Qt.binding(() => rail.cellY(index))
            }
        }
    }
}
