import QtQuick
import "../Theme"
import "../Primitives"

// A chosen filter in the Library breadcrumb: its column, its value, and an
// x that removes it (design/03 FilterCrumb).
Rectangle {
    id: crumb

    property string name
    property string value

    signal removed()

    implicitWidth: row.implicitWidth + Metrics.gap10 + Metrics.gap4
    implicitHeight: Metrics.crumbHeight
    radius: height / 2 // derived: a pill
    color: Theme.color.stateSelected

    Row {
        id: row
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4

        Text { anchors.verticalCenter: parent.verticalCenter; text: crumb.name; font.pointSize: Type.meta; color: Theme.color.textTertiary }
        Text { anchors.verticalCenter: parent.verticalCenter; text: crumb.value; font.pointSize: Type.meta; font.weight: Type.medium; color: Theme.color.textPrimary }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.x
            size: Metrics.iconSm
            color: Theme.color.textSecondary

            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: crumb.removed() }
        }
    }
}
