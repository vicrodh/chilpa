import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The virtual folder root, a lazy tree, and a flat whole-tree search.
Rectangle {
    id: tree

    property string activePath

    radius: Metrics.radiusMenu
    color: Theme.color.surfacePanel

    Edge {}

    FolderModel {
        id: roots
        path: ""
        query: find.text
    }

    SearchField {
        id: find
        x: Metrics.gap8
        y: Metrics.gap8
        width: parent.width - Metrics.gap16 // derived: padding on both sides
        placeholder: I18n.tr("Filter folders", I18n.rev)
        onEscaped: text = ""
    }

    Item {
        id: virtualRoot
        anchors.top: find.bottom
        anchors.topMargin: Metrics.gap6
        width: parent.width
        height: Metrics.folderTreeRow

        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: Metrics.gap4
            anchors.rightMargin: Metrics.gap4
            radius: Metrics.radiusItem
            color: tree.activePath === "" ? Theme.color.stateSelected : rootHover.hovered ? Theme.color.stateHover : "transparent"
        }
        Icon {
            id: rootIcon
            x: Metrics.gap10
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.folderOpen
            size: Metrics.iconSm
            color: tree.activePath === "" ? Theme.color.accent : Theme.color.textSecondary
        }
        Text {
            anchors.left: rootIcon.right
            anchors.leftMargin: Metrics.gap6
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("All folders", I18n.rev)
            font.pointSize: Type.small
            font.weight: tree.activePath === "" ? Type.medium : Type.regular
            color: Theme.color.textPrimary
        }
        HoverHandler { id: rootHover }
        TapHandler { onTapped: Navigation.openFolder("") }
    }

    Flickable {
        anchors.top: virtualRoot.bottom
        anchors.topMargin: Metrics.gap4
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap8
        width: parent.width
        clip: true
        contentWidth: width
        contentHeight: find.text === "" ? branches.height : matches.height
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: branches
            width: parent.width
            visible: find.text === ""

            Repeater {
                model: roots
                FolderTreeBranch {
                    activePath: tree.activePath
                }
            }
        }

        Column {
            id: matches
            width: parent.width
            visible: find.text !== ""

            Repeater {
                model: roots
                Item {
                    required property int index
                    required property string name
                    required property string path
                    width: matches.width
                    height: Metrics.folderTreeRow

                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: Metrics.gap4
                        anchors.rightMargin: Metrics.gap4
                        radius: Metrics.radiusItem
                        color: resultHover.hovered ? Theme.color.stateHover : "transparent"
                    }
                    Icon {
                        id: resultIcon
                        x: Metrics.gap10
                        anchors.verticalCenter: parent.verticalCenter
                        glyph: Icons.folder
                        size: Metrics.iconSm
                        color: Theme.color.textSecondary
                    }
                    Text {
                        anchors.left: resultIcon.right
                        anchors.leftMargin: Metrics.gap6
                        anchors.right: parent.right
                        anchors.rightMargin: Metrics.gap8
                        anchors.verticalCenter: parent.verticalCenter
                        text: name
                        elide: Text.ElideMiddle
                        font.pointSize: Type.micro
                        color: Theme.color.textPrimary
                    }
                    HoverHandler { id: resultHover }
                    TapHandler { onTapped: Navigation.openFolder(path) }
                }
            }
        }
    }
}
