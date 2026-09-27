import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One horizontal shelf of albums. It keeps eight candidates and scrolls
// naturally when the window cannot show them all.
Item {
    id: root

    property string title
    property AlbumGridModel model

    implicitHeight: heading.height + Metrics.gap12 + Metrics.carouselCard
                    + Metrics.cardTextGap + Metrics.cardTextHeight
    visible: model && model.albums > 0

    // The Home's section title, which is the ceiling for any title in a page.
    SectionHeader {
        id: heading
        width: parent.width
        title: root.title
    }

    ListView {
        id: rail
        anchors.top: heading.bottom
        anchors.topMargin: Metrics.gap12
        width: parent.width
        height: parent.height - y
        orientation: ListView.Horizontal
        spacing: Metrics.gridGap
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: root.model ? root.model.albums : 0
        ScrollBar.horizontal: ScrollBar {}

        delegate: AlbumCard {
            required property int index
            readonly property int revision: root.model.rev
            width: Metrics.carouselCard
            albumId: root.model.albumId(index, revision)
            title: root.model.title(index, revision)
            subtitle: root.model.subtitle(index, revision)
            year: root.model.year(index, revision)
            art: root.model.art(index, revision)
            tint: index
            canExpand: false
            onClicked: Navigation.go("album", albumId)
            onMenu: (x, y) => {
                const at = mapToItem(root, x, y)
                menu.show(albumId, title, subtitle, index, at.x, at.y)
            }
        }
    }

    AlbumMenu { id: menu }
}
