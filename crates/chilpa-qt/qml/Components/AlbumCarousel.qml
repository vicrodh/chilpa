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
    readonly property real farthest: Math.max(0, grid.scrollView.contentWidth - grid.scrollView.width)
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
        canBack: grid.scrollView.contentX > 1
        canForward: grid.scrollView.contentX < root.farthest - 1
        onBack: grid.scrollView.contentX = Math.max(0, grid.scrollView.contentX - root.step)
        onForward: grid.scrollView.contentX = Math.min(root.farthest, grid.scrollView.contentX + root.step)
        onLinkClicked: root.linkClicked()
    }
    AlbumGrid {
        id: grid
        Component.onCompleted: scrollView.objectName = "albumCarouselRail"
        width: parent.width
        height: implicitHeight + Metrics.gap8
        fitContent: true; fixedColumns: root.size || root.viewportColumns
        round: root.round
        viewport: root.viewport
        model: root.model
        onOpen: (albumId) => Navigation.go("album", albumId)
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
