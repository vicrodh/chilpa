import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// A shelf of records inside a page: a title and one row of the same grid as
// every other, so a record on a shelf has all a record has anywhere: its
// buttons, its menu, being chosen, and its quick view under the row.
Column {
    id: root

    property string title
    property AlbumGridModel model
    // A link on the right of the title ("See all").
    property string link
    // Artists: round portraits.
    property bool round: false

    signal linkClicked()

    visible: model && model.albums > 0
    spacing: Metrics.gap12

    SectionHeader {
        width: parent.width
        title: root.title
        link: root.link
        onLinkClicked: root.linkClicked()
    }
    AlbumGrid {
        id: grid
        width: parent.width
        height: implicitHeight
        fitContent: true
        round: root.round
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
