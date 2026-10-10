import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The one track list (frontend ADR-0006): configured by properties, never
// forked. Rows are recycled; hover and clicks are resolved here, once for
// the whole list, so a row carries no mouse handling of its own.
Item {
    id: list

    property TrackListModel model
    property bool rich: SettingsModel.density === "rich"
    property bool showArtist: true
    property bool showAlbum: true
    property bool showSource: model && model.kind !== "album" && model.kind !== "source"
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
    property bool groupActionsEnabled: true

    signal sortRequested(string order)

    readonly property alias contentHeight: view.contentHeight
    // The list itself, for a screen that groups its rows or jumps in them.
    readonly property alias rows: view

    implicitHeight: (header ? Metrics.rowHeader : 0) + view.contentHeight

    // Where the list was when its model is read again: the row at the top
    // and how far into it. A view goes back to its top on a reset, and a
    // queue is read again on every change; the same list stays where it was.
    property var place: null
    function identity() { return model ? model.kind + "\n" + model.albumId + "\n" + model.key : "" }
    function keepPlace() {
        const row = view.indexAt(0, view.contentY)
        const item = view.itemAtIndex(row)
        place = item ? { list: identity(), row: row, into: view.contentY - item.y } : null
    }
    function restorePlace() {
        const was = place
        place = null
        if (!was || was.list !== identity() || view.count === 0)
            return
        view.forceLayout()
        if (was.row >= view.count) {
            view.positionViewAtEnd()
            return
        }
        view.positionViewAtIndex(was.row, ListView.Beginning)
        view.contentY += was.into
        view.returnToBounds()
    }
    Connections {
        target: list.model
        function onModelAboutToBeReset() { list.keepPlace() }
        // After the view has taken the reset in.
        function onModelReset() { Qt.callLater(list.restorePlace) }
    }

    TrackColumns {
        id: columns
        width: list.width
        rich: list.rich
        showArtist: list.showArtist
        showAlbum: list.showAlbum
        showSource: list.showSource
        showArt: list.showArt
        showPlays: list.showPlays
        showRating: list.showRating
        reorder: list.reorderable && !list.selectionMode
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
        property int albumHovered: -1
        property int ratingHovered: 0
        // 1 the heart, 2 the menu button, of the row under the pointer.
        property int actionHovered: 0
        // 1 the arrow up, 2 the arrow down, of the row under the pointer.
        property int moveHovered: 0

        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        interactive: !list.fitContent
        clip: !list.fitContent
        model: list.model
        reuseItems: true
        // Off the screen a current row and a pooled one stay alive: the playing mark would move there (QBZ).
        currentIndex: -1
        // A list that grows with its rows builds them all; one that scrolls
        // keeps a screen of them ready on each side.
        cacheBuffer: list.fitContent ? Metrics.fitContentBuffer : Math.max(0, height)
        bottomMargin: list.bottomInset
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { policy: list.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded }

        delegate: TrackRow {
            required property var model
            sourceName: model.sourceName || ""
            composer: model.composer || ""
            sourceInfo: JSON.parse(model.sourceInfo || "{}")
            albumHovered: index === view.albumHovered
            cols: columns
            canRate: trackId !== 0
            rating: Ratings.stars("track", trackId, Ratings.rev)
            favorite: Ratings.favorite("track", trackId, Ratings.rev)
            ratingPreview: index === view.hovered ? view.ratingHovered : 0
            hovered: index === view.hovered
            actionHovered: index === view.hovered ? view.actionHovered : 0
            moveHovered: index === view.hovered ? view.moveHovered : 0
            artistHovered: index === view.artistHovered
            selectionMode: list.selectionMode
            ListView.onPooled: visible = false; ListView.onReused: visible = true
        }

        onContentYChanged: pointer.track()
    }

    TrackListPointer {
        id: pointer
        anchors.fill: view
        anchors.rightMargin: list.fitContent ? 0 : Metrics.gap10
        host: list
        rows: view
        cols: columns
        menu: actions
        measure: credit
    }

    // Where what is dragged would land.
    Rectangle {
        objectName: "dropLine"
        readonly property Item target: pointer.dropRow >= 0 && pointer.dropRow !== pointer.pressedRow ? view.itemAtIndex(pointer.dropRow) : null

        visible: !!target && !target.header
        x: columns.pad
        y: view.y + (target ? target.y - view.contentY + (pointer.dropAfter ? target.height : 0) : 0) - height / 2
        z: 4
        width: list.width - columns.pad * 2 // derived: padding on both sides
        height: Metrics.gap2
        radius: Metrics.gap2
        color: Theme.color.accent
    }

    DragGhost {
        objectName: "dragGhost"
        visible: pointer.dragging
        source: pointer
        at: Qt.point(pointer.mouseX, pointer.mouseY)
        count: pointer.dragCount
        title: pointer.dragTitle
        detail: pointer.dragDetail
    }

    // Measures where each artist of a row's credit is; one for the list.
    CreditLine { id: credit; visible: false }

    TrackMenu {
        id: actions
        model: list.model
    }

    TrackGroupActions {
        id: groupActions
        row: view.hovered
        item: row >= 0 ? view.itemAtIndex(row) : null
        model: list.model

        visible: list.groupActionsEnabled && item && item.header
        x: list.width - width - Metrics.gap10
        y: head.height + (item ? item.y - view.contentY + (item.height - height) / 2 : 0)
        z: 3
    }}
