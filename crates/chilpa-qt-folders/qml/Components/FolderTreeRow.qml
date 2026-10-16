// hot-path: one per row of the folder tree, recycled while it scrolls.
import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One row of the folder tree: a folder with its arrow, or a file, as deep
// as it is. The arrow opens and closes; the rest of the row is the list's.
Item {
    id: row

    required property int index
    required property string name
    required property string path
    required property int count
    required property int depth
    required property bool open
    required property bool file

    property bool active: false
    property bool hovered: false
    property bool selecting: false
    // While choosing several: 1 chosen, 2 holding something chosen, 0 neither.
    property int chosen: 0

    // Where the arrow is, for the list to tell a click on it.
    readonly property real arrowLeft: turn.x
    readonly property real arrowRight: glyph.x

    height: Metrics.folderTreeRow

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: Metrics.gap4
        anchors.rightMargin: Metrics.gap4
        radius: Metrics.radiusItem
        color: row.active ? Theme.color.stateSelected : row.hovered && !row.file ? Theme.color.stateHover : "transparent"
    }
    Icon {
        id: mark
        visible: row.selecting && !row.file
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.chosen === 1 ? Icons.squareCheckFilled : row.chosen === 2 ? Icons.minus : Icons.square
        filled: row.chosen === 1
        size: Metrics.iconSm
        color: row.chosen === 0 ? Theme.color.textSecondary : Theme.color.accent
    }
    Icon {
        id: turn
        visible: !row.file && row.count > 0
        x: Metrics.gap8 + (row.selecting ? Metrics.iconSm + Metrics.gap6 : 0) + row.depth * Metrics.folderTreeIndent
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronRight
        size: Metrics.iconXs
        color: Theme.color.textTertiary
        rotation: row.open ? 90 : 0
    }
    Icon {
        id: glyph
        x: turn.x + Metrics.iconXs + Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.file ? Icons.music : row.open ? Icons.folderOpen : Icons.folder
        size: Metrics.iconSm
        color: row.file ? Theme.color.textTertiary : Theme.color.accent
    }
    Text {
        anchors.left: glyph.right
        anchors.leftMargin: Metrics.gap6
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: row.name
        elide: Text.ElideMiddle
        font.pointSize: Type.small
        font.weight: row.active ? Type.medium : Type.regular
        color: row.file ? Theme.color.textSecondary : Theme.color.textPrimary
    }
}
