import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The one track list (frontend ADR-0006): configured by properties, never
// forked. Rows are recycled; hover and clicks are resolved here, once for
// the whole list, so a row carries no mouse handling of its own.
Item {
    id: list

    property TrackListModel model
    property bool rich: SettingsModel.density === "rich"
    property bool showArtist: true
    property bool showAlbum: true
    property bool showArt: true
    property bool showPlays: false
    property bool showRating: true
    property bool header: true
    // Grow with the rows instead of scrolling: a list inside a page that
    // scrolls as a whole.
    property bool fitContent: false
    // Room below the last row, for what floats over the list.
    property int bottomInset: 0
    // Titles that reorder the list, and the order in use.
    property bool sortable: false
    property string order
    // Playlist-only affordances. Multi-select is opt-in; custom order is the
    // only order that may be dragged or moved with arrows.
    property bool selectionMode: false
    property bool reorderable: false

    signal sortRequested(string order)

    readonly property alias contentHeight: view.contentHeight

    implicitHeight: (header ? Metrics.rowHeader : 0) + view.contentHeight

    TrackColumns {
        id: columns
        width: list.width
        rich: list.rich
        showArtist: list.showArtist
        showAlbum: list.showAlbum
        showArt: list.showArt
        showPlays: list.showPlays
        showRating: list.showRating
    }

    TrackHeader {
        id: head
        visible: list.header
        height: list.header ? Metrics.rowHeader : 0
        cols: columns
        sortable: list.sortable
        order: list.order
        onSortRequested: (order) => list.sortRequested(order)
    }

    ListView {
        id: view

        property int hovered: -1

        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        interactive: !list.fitContent
        clip: !list.fitContent
        model: list.model
        reuseItems: true
        // A list that grows with its rows builds them all; one that scrolls
        // keeps a screen of them ready on each side.
        cacheBuffer: list.fitContent ? Metrics.fitContentBuffer : Math.max(0, height)
        bottomMargin: list.bottomInset
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: list.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded }

        delegate: TrackRow {
            cols: columns
            hovered: index === view.hovered
            selectionMode: list.selectionMode
        }

        onContentYChanged: pointer.track()
    }

    MouseArea {
        id: pointer

        property int pressedRow: -1
        property real pressedY: 0
        property bool dragging: false

        anchors.fill: view
        anchors.rightMargin: list.fitContent ? 0 : Metrics.gap10
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton

        function rowAt(y) {
            return view.indexAt(Metrics.gap2, y + view.contentY)
        }
        function track() {
            view.hovered = containsMouse ? rowAt(mouseY) : -1
        }

        onPressed: (mouse) => {
            if (mouse.button === Qt.LeftButton && list.reorderable) {
                pressedRow = rowAt(mouse.y)
                pressedY = mouse.y
                dragging = false
            }
        }
        onPositionChanged: (mouse) => {
            track()
            if (pressedRow >= 0 && Math.abs(mouse.y - pressedY) > Qt.styleHints.startDragDistance)
                dragging = true
        }
        onReleased: (mouse) => {
            if (dragging && pressedRow >= 0) {
                const target = rowAt(mouse.y)
                if (target >= 0)
                    list.model.moveInPlaylistTo(pressedRow, target)
            }
            pressedRow = -1
            dragging = false
        }
        onContainsMouseChanged: track()
        onClicked: (mouse) => {
            const row = rowAt(mouse.y)
            if (row < 0)
                return
            if (mouse.button === Qt.RightButton) {
                list.model.select(row, 4)
                actions.show(row, mouse.x, view.y + mouse.y)
                return
            }
            const toggle = list.selectionMode || (mouse.modifiers & Qt.ControlModifier) || (mouse.modifiers & Qt.MetaModifier)
            list.model.select(row, (toggle ? 1 : 0) | (mouse.modifiers & Qt.ShiftModifier ? 2 : 0))
        }
        onDoubleClicked: (mouse) => {
            const row = rowAt(mouse.y)
            if (row >= 0 && mouse.button === Qt.LeftButton)
                list.model.play(row)
        }
    }

    TrackMenu {
        id: actions
        model: list.model
    }

    Row {
        id: groupActions
        readonly property int row: view.hovered
        readonly property var item: row >= 0 ? view.itemAtIndex(row) : null

        visible: item && item.header
        x: list.width - width - Metrics.gap10
        y: head.height + (item ? item.y - view.contentY + (item.height - height) / 2 : 0)
        spacing: Metrics.gap2
        z: 3

        IconButton {
            size: Metrics.button24
            glyph: Icons.playerPlayFilled
            filled: true
            tip: I18n.tr("Play this group", I18n.rev)
            onClicked: list.model.groupAction(groupActions.row, "play", groupActions.item.headerLabel)
        }
        IconButton {
            id: groupQueueButton
            size: Metrics.button24
            glyph: Icons.playlistAdd
            tip: I18n.tr("Queue this group", I18n.rev)
            onClicked: {
                const at = groupQueueButton.mapToItem(list, groupQueueButton.width - Metrics.menuWidth,
                                                      groupQueueButton.height + Metrics.gap4)
                groupQueue.showGroup(groupActions.row, groupActions.item.headerLabel, at.x, at.y)
            }
        }
    }

    QueueMenu {
        id: groupQueue
        model: list.model
    }

    Row {
        visible: list.reorderable && view.hovered >= 0 && (!groupActions.item || !groupActions.item.header)
        x: list.width - width - Metrics.gap10
        y: {
            const item = view.itemAtIndex(view.hovered)
            return head.height + (item ? item.y - view.contentY + (item.height - height) / 2 : 0)
        }
        spacing: Metrics.gap2
        z: 2

        IconButton {
            size: Metrics.button24
            glyph: Icons.chevronUp
            tip: I18n.tr("Move up", I18n.rev)
            onClicked: list.model.moveInPlaylist(view.hovered, -1)
        }
        IconButton {
            size: Metrics.button24
            glyph: Icons.chevronDown
            tip: I18n.tr("Move down", I18n.rev)
            onClicked: list.model.moveInPlaylist(view.hovered, 1)
        }
    }
}
