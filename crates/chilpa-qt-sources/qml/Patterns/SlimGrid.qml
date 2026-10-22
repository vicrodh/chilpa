import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A Home rail as QBZ's compact grid (SlimCarousel): four rows across and
// three down, a page of twelve and two pages at most, paged a whole width
// at a time by the header's arrows. The rail puts its rows in `cells` and
// places each with `cellX` and `cellY`; a row past the last page hides.
Column {
    id: grid
    property string title
    // How many rows the rail has.
    property int total: 0
    property alias cells: pages.flickableData
    readonly property int perPage: Metrics.slimColumns * Metrics.slimRows
    readonly property int shown: Math.min(total, Metrics.slimPages * perPage)
    readonly property int pageCount: Math.max(1, Math.ceil(shown / perPage))
    readonly property real maxScroll: (pageCount - 1) * pages.width
    readonly property real cellWidth: (width - (Metrics.slimColumns - 1) * Metrics.gap8) / Metrics.slimColumns // derived: four columns and the gaps between
    function cellX(index) { return Math.floor(index / perPage) * pages.width + (index % perPage % Metrics.slimColumns) * (cellWidth + Metrics.gap8) }
    function cellY(index) { return Math.floor(index % perPage / Metrics.slimColumns) * (Metrics.slimRow + Metrics.gap8) }
    spacing: Metrics.gap12

    SectionHeader {
        width: parent.width
        title: grid.title
        paged: grid.maxScroll > 0
        canBack: pages.contentX > 1
        canForward: pages.contentX < grid.maxScroll - 1
        onBack: pages.contentX = Math.max(0, pages.contentX - pages.width)
        onForward: pages.contentX = Math.min(grid.maxScroll, pages.contentX + pages.width)
    }
    Flickable {
        id: pages
        objectName: "slimGridPages"
        width: parent.width
        height: Metrics.slimRows * Metrics.slimRow + (Metrics.slimRows - 1) * Metrics.gap8 // derived: three rows and the gaps between
        contentWidth: grid.pageCount * width
        contentHeight: height
        clip: true
        interactive: grid.pageCount > 1
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
    }
}
