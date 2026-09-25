import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"

// One of the Library screen's four filter columns (design/03 PaneColumn):
// a header with its count, "All (n)" first, then the entries.
Rectangle {
    id: pane

    property string title
    // "year", "genre", "artist" or "album".
    property string column

    color: Theme.color.surfacePane

    FacetModel {
        id: facets
        column: pane.column
        rev: Library.rev
    }

    Item {
        id: head
        width: parent.width
        height: Metrics.paneHeader

        Text {
            anchors.left: parent.left
            anchors.leftMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter
            text: pane.title
            font.pointSize: Type.label
            font.weight: Type.semibold
            color: Theme.color.textSecondary
        }
        Text {
            anchors.right: parent.right
            anchors.rightMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter
            text: facets.total
            font.pointSize: Type.label
            color: Theme.color.textTertiary
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.color.strokeHairline
        }
    }

    ListView {
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        anchors.topMargin: Metrics.gap4
        width: parent.width
        clip: true
        model: facets
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        delegate: PaneRow {
            width: ListView.view.width
            onChosen: (key, label) => Library.choose(pane.column, key, label)
        }
    }
}
