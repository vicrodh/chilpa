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
        objectName: "trackListView"

        property int hovered: -1
        // The row whose artist is under the pointer, which a click opens.
        property int artistHovered: -1
        property int ratingHovered: 0

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
            required property double trackId
            cols: columns
            canRate: trackId > 0
            rating: Ratings.track(trackId, Ratings.rev)
            ratingPreview: index === view.hovered ? view.ratingHovered : 0
            hovered: index === view.hovered
            artistHovered: index === view.artistHovered
            selectionMode: list.selectionMode
        }

        onContentYChanged: pointer.track()
    }

    MouseArea {
        id: pointer
        cursorShape: Qt.PointingHandCursor

        property int pressedRow: -1
        property real pressedY: 0
        property bool dragging: false
        property bool pressedRating: false

        anchors.fill: view
        anchors.rightMargin: list.fitContent ? 0 : Metrics.gap10
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton

        function rowAt(y) {
            return view.indexAt(Metrics.gap2, y + view.contentY)
        }
        function track() {
            view.hovered = containsMouse ? rowAt(mouseY) : -1
            view.artistHovered = containsMouse && onArtist(view.hovered, mouseX, mouseY) ? view.hovered : -1
            view.ratingHovered = ratingAt(view.hovered, mouseX, mouseY)
        }
        function ratingAt(row, x, y) {
            const item = row >= 0 ? view.itemAtIndex(row) : null
            return item ? item.ratingAt(x - item.x, y + view.contentY - item.y) : 0
        }
        // Whether the pointer is on the row's artist: its own column in
        // compact rows, the line under the title in rich ones. Worked out
        // here, once for the list, and not by a handler on every row.
        function onArtist(row, x, y) {
            const item = row >= 0 ? view.itemAtIndex(row) : null
            if (!item || item.header || item.artist === "" || !list.model.canGo(row, "artist"))
                return false
            return item.containsArtist(x - item.x, y + view.contentY - item.y)
        }

        onPressed: (mouse) => {
            pressedRating = ratingAt(rowAt(mouse.y), mouse.x, mouse.y) > 0
            if (mouse.button === Qt.LeftButton && list.reorderable && !pressedRating) {
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
            const star = ratingAt(row, mouse.x, mouse.y)
            if (pressedRating && star === 0)
                return
            if (star > 0) {
                const item = view.itemAtIndex(row)
                Ratings.rateTrack(item.trackId, item.rating === star ? 0 : star)
                return
            }
            const toggle = list.selectionMode || (mouse.modifiers & Qt.ControlModifier) || (mouse.modifiers & Qt.MetaModifier)
            if (!toggle && !(mouse.modifiers & Qt.ShiftModifier) && onArtist(row, mouse.x, mouse.y)) {
                list.model.go(row, "artist")
                return
            }
            list.model.select(row, (toggle ? 1 : 0) | (mouse.modifiers & Qt.ShiftModifier ? 2 : 0))
        }
        onDoubleClicked: (mouse) => {
            const row = rowAt(mouse.y)
            if (row >= 0 && mouse.button === Qt.LeftButton && !pressedRating && !ratingAt(row, mouse.x, mouse.y))
                list.model.play(row)
        }
    }

    TrackMenu {
        id: actions
        model: list.model
    }

    TrackGroupActions {
        id: groupActions
        row: view.hovered
        item: row >= 0 ? view.itemAtIndex(row) : null
        model: list.model

        visible: item && item.header
        x: list.width - width - Metrics.gap10
        y: head.height + (item ? item.y - view.contentY + (item.height - height) / 2 : 0)
        z: 3
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
