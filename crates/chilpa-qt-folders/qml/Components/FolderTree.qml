import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.folders
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

// The library's folders as a tree: one list of rows, each as deep as it is,
// so closing a branch takes its rows out and leaves nothing behind. It has
// a search across all of it, a way to close every branch at once, and a
// way to choose several folders.
Rectangle {
    id: tree

    property string activePath
    property FolderMenu menu
    // Choosing several: the folders chosen, and a mark that moves with them.
    property bool selecting: false
    property var chosen: ({})
    property int chosenRev: 0
    readonly property var chosenPaths: Object.keys(chosen)
    // Where the list was before its rows were read again.
    property real heldAt: 0

    // 1 when the folder or one above it is chosen, 2 when one under it is.
    function chosenAt(path, rev) {
        if (chosenPaths.some((one) => path === one || path.startsWith(one + "/")))
            return 1
        return chosenPaths.some((one) => one.startsWith(path + "/")) ? 2 : 0
    }
    function choose(path) {
        const next = Object.assign({}, chosen)
        if (next[path]) delete next[path]
        else next[path] = true
        chosen = next
        chosenRev += 1
    }
    function release() {
        chosen = ({})
        chosenRev += 1
    }
    function act(action) {
        const paths = action === "next" ? chosenPaths.slice().reverse() : chosenPaths
        paths.forEach((path) => tree.menu.folder.act(path, action))
    }
    function turn(path) {
        heldAt = view.contentY
        rows.turn(path)
    }

    radius: Metrics.radiusMenu
    color: Theme.color.surfacePanel
    onActivePathChanged: rows.reveal(activePath)
    Component.onCompleted: rows.reveal(activePath)

    Edge {}

    FolderModel {
        id: rows
        tree: true
        contentRev: Library.contentRev
        query: sought.text
        onRevChanged: view.contentY = Math.max(0, Math.min(tree.heldAt, view.contentHeight - view.height))
    }
    // What is typed is looked for once the typing stops.
    Timer {
        id: settle
        interval: Motion.treeSettle
        onTriggered: sought.text = find.text.trim()
    }
    QtObject { id: sought; property string text }

    IconButton {
        id: select
        x: Metrics.gap8
        anchors.verticalCenter: find.verticalCenter
        tip: I18n.tr("Select folders", I18n.rev)
        glyph: Icons.checkbox
        checked: tree.selecting
        onClicked: {
            tree.selecting = !tree.selecting
            tree.release()
        }
    }
    IconButton {
        id: close
        anchors.left: select.right
        anchors.verticalCenter: find.verticalCenter
        tip: I18n.tr("Collapse all", I18n.rev)
        glyph: Icons.chevronUp
        onClicked: tree.turn("")
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
    FolderBulkBar {
        id: bulk
        visible: tree.selecting && tree.chosenPaths.length > 0
        anchors.top: find.bottom
        anchors.topMargin: Metrics.gap6
        x: Metrics.gap8
        width: parent.width - Metrics.gap16 // derived: padding on both sides
        height: visible ? implicitHeight : 0
        count: tree.chosenPaths.length
        onActed: (action) => tree.act(action)
        onCleared: tree.release()
    }

    ListView {
        id: view

        property int hovered: -1

        anchors.top: bulk.visible ? bulk.bottom : find.bottom
        anchors.topMargin: Metrics.gap6
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap8
        width: parent.width
        clip: true
        model: rows
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}

        delegate: FolderTreeRow {
            width: view.width
            active: !file && tree.activePath === path
            hovered: index === view.hovered
            selecting: tree.selecting
            chosen: tree.selecting && !file ? tree.chosenAt(path, tree.chosenRev) : 0
        }
    }
    // One pointer for the list, not a handler on every row.
    MouseArea {
        anchors.fill: view
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: view.hovered >= 0 ? Qt.PointingHandCursor : Qt.ArrowCursor

        function rowAt(y) {
            const at = view.indexAt(Metrics.gap2, y + view.contentY)
            const item = at >= 0 ? view.itemAtIndex(at) : null
            return item && !item.file ? item : null
        }
        onPositionChanged: (mouse) => { const item = rowAt(mouse.y); view.hovered = item ? item.index : -1 }
        onExited: view.hovered = -1
        onClicked: (mouse) => {
            const item = rowAt(mouse.y)
            if (!item)
                return
            if (mouse.button === Qt.RightButton) {
                const at = mapToItem(tree.menu.parent, mouse.x, mouse.y)
                tree.menu.show(item.path, at.x, at.y)
            } else if (item.count > 0 && mouse.x >= item.arrowLeft && mouse.x < item.arrowRight)
                tree.turn(item.path)
            else if (tree.selecting)
                tree.choose(item.path)
            else
                Navigation.openFolder(item.path)
        }
    }
    Text {
        visible: rows.folders === 0 && !rows.loading
        anchors.top: view.top
        x: Metrics.gap10
        width: parent.width - Metrics.gap16 // derived: padding on both sides
        topPadding: Metrics.gap8
        wrapMode: Text.Wrap
        text: sought.text === "" ? I18n.tr("No folders in your local library yet.", I18n.rev) : I18n.tr("No folders match your search.", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
