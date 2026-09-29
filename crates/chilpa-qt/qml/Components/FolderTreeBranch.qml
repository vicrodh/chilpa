import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One branch of the folder tree. A closed branch does not ask for what is
// inside it. Its arrow opens and closes it; a click on it selects it, which
// shows what it holds beside the tree and leaves the branch as it was.
Column {
    id: branch

    required property string name
    required property string path
    property string activePath
    property int depth: 0
    property int closed: 0
    property var rail
    // Its arrow was used: it is open or closed because somebody said so.
    property bool turned: false
    property bool expanded: false

    // A root is kept with the slash it was added with.
    readonly property string place: path.replace(/\/+$/, "")
    readonly property bool active: activePath === place
    // The folder that is open is always in sight, until its branch is closed.
    readonly property bool leadsToActive: activePath.startsWith(place + "/")
    readonly property bool opened: turned ? expanded : leadsToActive

    onClosedChanged: {
        turned = true
        expanded = false
    }

    width: parent ? parent.width : 0

    Item {
        width: branch.width
        height: Metrics.folderTreeRow

        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: Metrics.gap4
            anchors.rightMargin: Metrics.gap4
            radius: Metrics.radiusItem
            color: branch.active ? Theme.color.stateSelected : hover.hovered ? Theme.color.stateHover : "transparent"
        }

        Icon {
            id: turn
            x: Metrics.gap8 + branch.depth * Metrics.folderTreeIndent
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.chevronRight
            size: Metrics.iconXs
            color: Theme.color.textTertiary
            rotation: branch.opened ? 90 : 0

            HoverHandler { cursorShape: Qt.PointingHandCursor }
            TapHandler {
                onTapped: {
                    branch.expanded = !branch.opened
                    branch.turned = true
                }
            }
        }
        Icon {
            id: folderIcon
            anchors.left: turn.right
            anchors.leftMargin: Metrics.gap4
            anchors.verticalCenter: parent.verticalCenter
            glyph: branch.opened ? Icons.folderOpen : Icons.folder
            size: Metrics.iconSm
            color: Theme.color.accent
        }
        Text {
            anchors.left: folderIcon.right
            anchors.leftMargin: Metrics.gap6
            anchors.right: parent.right
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            text: branch.name
            elide: Text.ElideMiddle
            font.pointSize: Type.small
            font.weight: branch.active ? Type.medium : Type.regular
            color: Theme.color.textPrimary
        }

        HoverHandler { id: hover }
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: Navigation.openFolder(branch.path) }
        TapHandler { acceptedButtons: Qt.RightButton; onTapped: (point) => branch.rail.context(branch, branch.path, point.position.x, point.position.y) }
    }

    Loader {
        width: branch.width
        active: branch.opened
        sourceComponent: Column {
            width: branch.width

            FolderModel { id: children; path: branch.path }
            Repeater {
                model: children

                Loader {
                    id: child
                    required property int index
                    required property string name
                    required property string path
                    property string activePath: branch.activePath
                    property int closed: branch.closed
                    width: branch.width
                    height: item ? item.implicitHeight : 0

                    Component.onCompleted: setSource("FolderTreeBranch.qml", {
                        name: child.name,
                        path: child.path,
                        activePath: child.activePath,
                        closed: child.closed,
                        rail: branch.rail,
                        depth: branch.depth + 1
                    })
                    onActivePathChanged: if (item) item.activePath = activePath
                    onClosedChanged: if (item) item.closed = closed
                }
            }
        }
    }
}
