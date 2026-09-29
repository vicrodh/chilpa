import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The library's folders as a tree that opens a level at a time, with a
// search across all of it and a way to close every branch at once.
Rectangle {
    id: tree

    property string activePath
    property FolderMenu menu
    // Moves when every branch is asked to close.
    property int closed: 0

    function context(item, path, left, top) {
        const at = item.mapToItem(tree.menu.parent, left, top)
        tree.menu.show(path, at.x, at.y)
    }

    radius: Metrics.radiusMenu
    color: Theme.color.surfacePanel

    Edge {}

    FolderModel {
        id: roots
        path: ""
        query: sought.text
    }
    // What is typed is looked for once the typing stops.
    Timer {
        id: settle
        interval: Motion.treeSettle
        onTriggered: sought.text = find.text.trim()
    }
    QtObject { id: sought; property string text }

    IconButton {
        id: close
        x: Metrics.gap8
        anchors.verticalCenter: find.verticalCenter
        tip: I18n.tr("Collapse all", I18n.rev)
        glyph: Icons.chevronUp
        onClicked: tree.closed += 1
    }
    SearchField {
        id: find
        anchors.left: close.right
        anchors.leftMargin: Metrics.gap6
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        y: Metrics.gap8
        placeholder: I18n.tr("Search folders", I18n.rev)
        onTextChanged: settle.restart()
        onEscaped: text = ""
    }

    Flickable {
        anchors.top: find.bottom
        anchors.topMargin: Metrics.gap6
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap8
        width: parent.width
        clip: true
        contentWidth: width
        contentHeight: sought.text === "" ? branches.height : matches.height
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        Column {
            id: branches
            width: parent.width
            visible: sought.text === ""

            Repeater {
                model: sought.text === "" ? roots : null
                FolderTreeBranch {
                    activePath: tree.activePath
                    closed: tree.closed
                    rail: tree
                }
            }
            Text {
                visible: roots.folders === 0 && !roots.loading
                x: Metrics.gap10
                topPadding: Metrics.gap8
                text: I18n.tr("No folders in your local library yet.", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }

        Column {
            id: matches
            width: parent.width
            visible: sought.text !== ""

            Repeater {
                model: sought.text !== "" ? roots : null
                Item {
                    id: match
                    required property string name
                    required property string path
                    width: matches.width
                    height: Metrics.folderTreeRow

                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: Metrics.gap4
                        anchors.rightMargin: Metrics.gap4
                        radius: Metrics.radiusItem
                        color: tree.activePath === match.path ? Theme.color.stateSelected : over.hovered ? Theme.color.stateHover : "transparent"
                    }
                    Icon {
                        id: glyph
                        x: Metrics.gap10
                        anchors.verticalCenter: parent.verticalCenter
                        glyph: Icons.folder
                        size: Metrics.iconSm
                        color: Theme.color.accent
                    }
                    Text {
                        anchors.left: glyph.right
                        anchors.leftMargin: Metrics.gap6
                        anchors.right: parent.right
                        anchors.rightMargin: Metrics.gap8
                        anchors.verticalCenter: parent.verticalCenter
                        text: match.name
                        elide: Text.ElideMiddle
                        font.pointSize: Type.small
                        color: Theme.color.textPrimary
                    }
                    HoverHandler { id: over; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Navigation.openFolder(match.path) }
                    TapHandler { acceptedButtons: Qt.RightButton; onTapped: (point) => tree.context(match, match.path, point.position.x, point.position.y) }
                }
            }
            Text {
                visible: roots.folders === 0 && !roots.loading
                x: Metrics.gap10
                topPadding: Metrics.gap8
                text: I18n.tr("No folders match your search.", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }
    }
}
