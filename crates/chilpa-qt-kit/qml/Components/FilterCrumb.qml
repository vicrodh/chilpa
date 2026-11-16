import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A chosen filter in the Library breadcrumb: its column, its value, and an
// x that removes it (design/03 FilterCrumb). Given a `room`, the value and
// then the column give way to it, each cut with an ellipsis.
Rectangle {
    id: crumb

    property string name
    property string value
    property real room: Number.POSITIVE_INFINITY
    // What is not words: the padding, the gaps and the x.
    readonly property real frame: Metrics.gap10 + Metrics.gap4 * 3 + remove.width // derived: the parts around the two texts
    readonly property real least: Metrics.gap16 // the narrowest a cut text gets: room for its ellipsis

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

        Text {
            id: key
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.max(crumb.least, crumb.room - crumb.frame - Math.min(shown.implicitWidth, crumb.least)))
            text: crumb.name
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
        Text {
            id: shown
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.max(crumb.least, crumb.room - crumb.frame - key.width))
            text: crumb.value
            elide: Text.ElideRight
            font.pointSize: Type.meta
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Icon {
            id: remove
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.x
            size: Metrics.iconSm
            color: Theme.color.textSecondary

            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: crumb.removed() }
        }
    }
}
