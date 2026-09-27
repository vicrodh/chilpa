import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

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
        query: input.text
    }

    Rectangle {
        id: head
        width: parent.width
        height: Metrics.paneHeader
        color: Theme.color.surfacePane

        Icon {
            id: lens
            anchors.left: parent.left
            anchors.leftMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.search
            size: Metrics.iconXs
            color: input.activeFocus ? Theme.color.accent : Theme.color.textTertiary
        }

        TextInput {
            id: input
            anchors.left: lens.right
            anchors.leftMargin: Metrics.gap6
            anchors.right: tail.left
            anchors.rightMargin: Metrics.gap6
            anchors.verticalCenter: parent.verticalCenter
            clip: true
            font.pointSize: Type.label
            color: Theme.color.textPrimary
            selectionColor: Theme.color.rowSelected

            Text {
                anchors.fill: parent
                visible: input.text === ""
                text: pane.title
                font: input.font
                color: Theme.color.textSecondary
                elide: Text.ElideRight
            }
        }

        Item {
            id: tail
            anchors.right: parent.right
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            width: input.text === "" ? count.implicitWidth : Metrics.iconSm
            height: parent.height

            Text {
                id: count
                anchors.centerIn: parent
                visible: input.text === ""
                text: facets.total
                font.pointSize: Type.micro
                color: Theme.color.textTertiary
            }
            Icon {
                anchors.centerIn: parent
                visible: input.text !== ""
                glyph: Icons.x
                size: Metrics.iconXs
                color: clear.hovered ? Theme.color.textPrimary : Theme.color.textTertiary

                HoverHandler { id: clear }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: input.text = "" }
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: input.activeFocus ? Metrics.focusRing : 1
            color: input.activeFocus ? Theme.color.accent : Theme.color.strokeHairline
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
            required property int index
            width: ListView.view.width
            onChosen: (modifiers) => facets.choose(index, modifiers)
        }
    }
}
