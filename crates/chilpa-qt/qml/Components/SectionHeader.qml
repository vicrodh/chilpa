import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The title of a shelf or a section, with an optional icon and a link on
// the right ("See all"). A shelf that scrolls sideways pages with two
// chevrons after the link, as QBZ's rails (QbzSectionHeader): each is
// enabled while there is more that way.
Item {
    id: head

    property string title
    property string glyph
    property string sourceIcon
    property string link
    property bool paged: false
    property bool canBack: false
    property bool canForward: false

    signal linkClicked()
    signal back()
    signal forward()

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
        SourceIcon {
            anchors.verticalCenter: parent.verticalCenter
            visible: head.sourceIcon !== ""
            sourceInfo: ({icon: head.sourceIcon})
            width: visible ? Metrics.iconSm : 0
            height: Metrics.iconSm
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

    Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4

        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: head.link !== ""
            rightPadding: head.paged ? Metrics.gap8 : 0
            text: head.link
            font.pointSize: Type.meta
            color: linkHover.hovered ? Theme.color.textPrimary : Theme.color.textSecondary

            HoverHandler { id: linkHover }
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: head.linkClicked() }
        }
        IconButton {
            objectName: "shelfBack"
            visible: head.paged
            anchors.verticalCenter: parent.verticalCenter
            round: true
            enabled: head.canBack
            glyph: Icons.chevronLeft
            tip: I18n.tr("Previous", I18n.rev)
            onClicked: head.back()
        }
        IconButton {
            objectName: "shelfForward"
            visible: head.paged
            anchors.verticalCenter: parent.verticalCenter
            round: true
            enabled: head.canForward
            glyph: Icons.chevronRight
            tip: I18n.tr("Next", I18n.rev)
            onClicked: head.forward()
        }
    }
}
