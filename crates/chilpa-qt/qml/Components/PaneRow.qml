// hot-path: one per entry of a filter column, recycled while it scrolls.
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/query/qml/Theme"

// An entry of a Library filter column: its label, and its count or year.
// A rating's entry draws its stars; "Unrated" and "All" stay words.
Item {
    id: row

    required property string key
    required property string label
    required property string meta
    required property bool excluded
    required property bool selected
    required property bool all
    property bool canExclude: true
    property bool stars: false
    readonly property bool drawn: stars && !all && key !== "0"

    // 1 for Ctrl or Cmd, 2 for Shift, 4 for Alt, which keeps the value out.
    signal chosen(int modifiers)
    signal exclusionChosen()

    height: Metrics.paneRow

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: Metrics.gap4
        anchors.rightMargin: Metrics.gap4
        radius: Metrics.radiusRow
        color: row.selected ? Theme.color.stateSelected : hover.hovered ? Theme.color.stateHover : "transparent"
    }

    Loader {
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        active: row.drawn
        opacity: row.excluded ? Metrics.disabledOpacity : 1
        sourceComponent: RatingStars { objectName: "paneStars"; value: Number(row.key); interactive: false }
    }
    Text {
        objectName: "paneLabel"
        visible: !row.drawn
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap12
        anchors.right: meta.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: row.label
        elide: Text.ElideRight
        font.pointSize: Type.row
        font.weight: row.selected ? Type.semibold : Type.regular
        font.strikeout: row.excluded
        color: row.excluded ? Theme.color.textSecondary : Theme.color.textPrimary
    }

    Text {
        id: meta
        anchors.right: row.canExclude ? exclusion.left : parent.right
        anchors.rightMargin: row.canExclude ? Metrics.gap8 : Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.meta
        font.pointSize: Type.label
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    Rectangle {
        id: exclusion
        objectName: "paneExclude"
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        width: Refine.columnExclude
        height: width
        radius: Refine.columnExcludeCorner
        visible: row.canExclude && !row.all
        color: excludeArea.containsMouse ? Theme.color.stateSelected : "transparent"
        Icon {
            anchors.centerIn: parent
            glyph: Icons.minus
            size: Refine.columnExcludeIcon
            color: row.excluded ? Theme.color.textPrimary : Theme.color.textTertiary
        }
        MouseArea {
            id: excludeArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: row.exclusionChosen()
        }
    }

    HoverHandler { id: hover }
    Item {
        width: row.all || !row.canExclude ? parent.width : exclusion.x
        height: parent.height
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler {
            id: tap
            onTapped: {
                const held = tap.point.modifiers
                row.chosen((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0) | (held & Qt.AltModifier ? 4 : 0))
            }
        }
    }
}
