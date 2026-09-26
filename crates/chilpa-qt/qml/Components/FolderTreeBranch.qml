import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One lazily opened branch of the folder tree. Closed branches do not query
// their children; the route's own ancestors open so the current folder stays
// visible when navigation came from a breadcrumb or track menu.
Column {
    id: branch

    required property string name
    required property string path
    property string activePath
    property int depth: 0
    property bool manuallyExpanded: false

    readonly property bool active: activePath === path
    readonly property bool activeWithin: active || activePath.startsWith(path + "/")
    readonly property bool opened: manuallyExpanded || activeWithin

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

            TapHandler { onTapped: branch.manuallyExpanded = !branch.opened }
        }
        Icon {
            id: folderIcon
            anchors.left: turn.right
            anchors.leftMargin: Metrics.gap4
            anchors.verticalCenter: parent.verticalCenter
            glyph: branch.opened ? Icons.folderOpen : Icons.folder
            size: Metrics.iconSm
            color: branch.active ? Theme.color.accent : Theme.color.textSecondary
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
        TapHandler {
            onTapped: {
                branch.manuallyExpanded = true
                Navigation.openFolder(branch.path)
            }
        }
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
                    width: branch.width
                    height: item ? item.implicitHeight : 0

                    Component.onCompleted: setSource("FolderTreeBranch.qml", {
                        name: child.name,
                        path: child.path,
                        activePath: child.activePath,
                        depth: branch.depth + 1
                    })
                    onActivePathChanged: if (item) item.activePath = activePath
                }
            }
        }
    }
}
