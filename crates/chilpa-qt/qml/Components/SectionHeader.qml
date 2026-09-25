import QtQuick
import "../Theme"
import "../Primitives"

// The title of a shelf or a section, with an optional icon and a link on
// the right ("See all").
Item {
    id: head

    property string title
    property string glyph
    property string link

    signal linkClicked()

    implicitHeight: Metrics.shelfHeader

    Row {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: head.glyph !== ""
            glyph: head.glyph
            color: Theme.color.textSecondary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: head.title
            font.pointSize: Type.section
            font.weight: Type.heading
            font.letterSpacing: Type.sectionTracking
            color: Theme.color.textPrimary
        }
    }

    Text {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: head.link !== ""
        text: head.link
        font.pointSize: Type.meta
        color: linkHover.hovered ? Theme.color.textPrimary : Theme.color.textSecondary

        HoverHandler { id: linkHover }
        TapHandler { onTapped: head.linkClicked() }
    }
}
