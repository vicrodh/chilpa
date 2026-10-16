import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A shelf of records inside a page: a title and one row of the same grid as
// every other, so a record on a shelf has all a record has anywhere: its
// buttons, its menu, being chosen, and its quick view under the row.
Column {
    id: root

    property Flickable viewport: null
    property string title
    property AlbumGridModel model
    // A link on the right of the title ("See all").
    property string link
    // Artists: round portraits.
    property bool round: false
    property int size: 0
    property string sourceIcon: ""
    readonly property int viewportColumns: CardLayout.columns(width)
    readonly property int cardCount: Math.max(1, Math.min(model ? model.albums : 0, size || viewportColumns))
    readonly property real farthest: Math.max(0, rail.contentWidth - rail.width)
    readonly property real step: viewportColumns * (CardLayout.cardWidth + Metrics.gridGap) // derived: a page of library-sized cards

    signal linkClicked()

    visible: model && model.albums > 0
    spacing: Metrics.gap12

    SectionHeader {
        width: parent.width
        title: root.title
        link: root.link
        sourceIcon: root.sourceIcon
        paged: root.farthest > 0
        canBack: rail.contentX > 1
        canForward: rail.contentX < root.farthest - 1
        onBack: rail.contentX = Math.max(0, rail.contentX - root.step)
        onForward: rail.contentX = Math.min(root.farthest, rail.contentX + root.step)
        onLinkClicked: root.linkClicked()
    }
    Flickable {
        id: rail
        objectName: "albumCarouselRail"
        width: parent.width
        height: Math.max(CardLayout.cardHeight + Metrics.cardRing * 2, grid.implicitHeight) + Metrics.gap8 // derived: a full cover row before ListView's first polish
        contentWidth: grid.width; contentHeight: height
        interactive: contentWidth > width
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.horizontal: ViewportScrollBar {}
        AlbumGrid {
            id: grid
            width: Metrics.cardRing * 2 + root.cardCount * CardLayout.cardWidth + (root.cardCount - 1) * Metrics.gridGap // derived: one row and its selection rings
            height: implicitHeight
            fitContent: true; fixedColumns: root.cardCount
            viewportX: rail.contentX; viewportWidth: rail.width
            round: root.round
            viewport: root.viewport
            model: root.model
            onOpen: (albumId) => Navigation.go("album", albumId)
        }
    }
    // One row: as many records as fit across.
    Binding {
        target: root.model
        property: "columns"
        value: grid.columns
    }
    Binding {
        target: root.model
        property: "maxRows"
        value: 1
    }
}
