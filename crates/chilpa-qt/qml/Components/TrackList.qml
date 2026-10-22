import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Tools
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
// The one track list (frontend ADR-0006): configured by properties, never
// forked. Rows are recycled; hover and clicks are resolved here, once for
// the whole list, so a row carries no mouse handling of its own.
Item {
    id: list

    readonly property bool viTrackList: true
    property alias cursorRow: viCursor.row
    activeFocusOnTab: true

    function viAction(action) { viCursor.run(action) }
    Tools.ViCursor { id: viCursor; host: list; view: view; menu: actions; ratings: Ratings }

    property TrackListModel model
    property bool rich: SettingsModel.density === "rich"
    property bool showArtist: true
    property bool showAlbum: true
    property bool showSource: model && model.kind !== "album" && model.kind !== "source"
    property bool showArt: true
    property bool showPlays: false
    property bool showRating: true
    property bool header: true
    // How far right of the rows the scrollbar sits: in the middle of the
    // page's margin, unless the screen puts something there.
    property real scrollGutter: Metrics.pageGutter
    // Grow with the rows instead of scrolling: a list inside a page that
    // scrolls as a whole.
    property bool fitContent: false
    // A page keeps row slots at full height and builds only nearby contents.
    property Flickable viewport: null
    property bool prefetch: true
    readonly property real visibleTop: viewport ? viewport.contentY - parent.mapToItem(viewport.contentItem, 0, y + view.y).y : view.contentY
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
    signal userScrollStarted()

    readonly property alias contentHeight: view.contentHeight
    // The list itself, for a screen that groups its rows or jumps in them.
    readonly property alias rows: view
    readonly property alias columns: columns

    implicitHeight: fitContent ? (header ? Metrics.rowHeader : 0) + view.contentHeight : 0

    // Where the list was when its model is read again: the row at the top
    // and how far into it. A view goes back to its top on a reset, and a
    // queue is read again on every change; the same list stays where it was.
    property alias place: keptViewport.place
    Tools.TrackListViewport { id: keptViewport; model: list.model; view: view }
    function positionAt(row, mode) {
        place = null
        view.forceLayout()
        view.positionViewAtIndex(row, mode)
    }
    // Goes to `row` with the view started again from the top. Through
    // section bands, a jump from far away held the window for 5 to 9 s and
    // landed short: the view walked the bands it had only estimated. The
    // rows it lets go are not pooled, or the top one came back showing a
    // row from below.
    function jumpTo(row) {
        place = null
        view.reuseItems = false
        view.model = null
        view.model = Qt.binding(() => list.model)
        view.forceLayout()
        view.positionViewAtIndex(row, ListView.Beginning)
        view.reuseItems = true
    }
    function identity() { return keptViewport.identity() }
    function keepPlace() { keptViewport.keep() }
    function restorePlace() { keptViewport.restore() }
    Connections {
        target: list.model
        function onModelAboutToBeReset() { list.keepPlace() }
        // After the view has taken the reset in.
        function onModelReset() {
            list.rowsRev++
            if (list.cursorRow >= view.count) list.cursorRow = -1
            Qt.callLater(list.restorePlace)
        }
    }

    // The rows' reset, for the columns to read their widest texts again.
    property int rowsRev: 0

    Tools.TrackColumns {
        id: columns
        width: list.width
        widest: !list.rich && list.model && typeof list.model.widest === "function"
            ? JSON.parse(list.model.widest(list.rowsRev + (list.model.count || 0)) || "{}") : ({})
        rich: list.rich
        showArtist: list.showArtist
        showAlbum: list.showAlbum
        showSource: list.showSource
        showArt: list.showArt
        showPlays: list.showPlays
        showRating: list.showRating
        reorder: list.reorderable && !list.selectionMode
        rows: list.model ? list.model.count : 0
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

    Tools.MotionListView {
        id: view
        motionViewport: list.viewport
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
        // A growing list retains its slots; the outer viewport windows their
        // contents. A standalone list caches one screen on each side, unless
        // it has section bands: then the view places far rows by an
        // estimate the bands throw off (4% on 22,000 tracks grouped by
        // name), and with a cache a jump drew its rows where the estimate
        // put them, off the screen, leaving it blank.
        cacheBuffer: list.fitContent ? Metrics.fitContentBuffer : view.section.property !== "" ? 0 : Math.max(0, height)
        bottomMargin: list.bottomInset
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {
            objectName: "trackListScrollbar"
            onPressedChanged: if (pressed) list.userScrollStarted()
            policy: list.fitContent ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded
            gutter: list.scrollGutter
            viewportHovered: pointer.containsMouse
        }

        delegate: TrackRow {
            required property var model
            immediate: y + height > list.visibleTop && y < list.visibleTop + (list.viewport ? list.viewport.height : view.height)
            inViewport: !list.viewport || immediate || list.prefetch && y + height >= list.visibleTop - list.viewport.height && y < list.visibleTop + list.viewport.height * 2
            sourceName: model.sourceName || ""
            composer: model.composer || ""
            headerGlyph: list.model && list.model.kind === "listenlist" ? (headerLabel === I18n.tr("History", I18n.rev) ? Lucide.rotateCcwClock : headerLabel === I18n.tr("Up next", I18n.rev) ? Lucide.listVideo : "") : ""
            headerIconFamily: headerGlyph !== "" ? Lucide.font : ""
            headerWork: model.headerWork || false
            headerComposer: model.headerComposer || ""
            headerComposerTarget: model.headerComposerTarget || ""
            sourceInfo: JSON.parse(model.sourceInfo || "{}")
            albumHovered: index === view.albumHovered
            cols: columns
            canRate: trackId !== 0
            rating: inViewport ? Ratings.stars("track", trackId, Ratings.rev) : 0
            favorite: inViewport && Ratings.favorite("track", trackId, Ratings.rev)
            ratingPreview: index === view.hovered ? view.ratingHovered : 0
            hovered: index === view.hovered || list.activeFocus && index === list.cursorRow
            actionHovered: index === view.hovered ? view.actionHovered : 0
            moveHovered: index === view.hovered ? view.moveHovered : 0
            artistHovered: index === view.artistHovered
            selectionMode: list.selectionMode
            groupActionsEnabled: list.groupActionsEnabled
            groupStart: ListView.section !== "" && ListView.section !== ListView.previousSection
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
        groupMenu: discMenu
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

    QueueMenu { id: discMenu; model: list.model; z: 3 }

    // Work and record groups keep the existing compact toolbar.
    TrackGroupActions {
        readonly property Item rowItem: view.hovered >= 0 ? view.itemAtIndex(view.hovered) : null
        visible: list.groupActionsEnabled && rowItem && rowItem.header && (rowItem.headerWork || rowItem.record)
        row: view.hovered
        item: rowItem
        model: list.model
        x: list.width - width - Metrics.gap12
        y: rowItem ? view.y + rowItem.y - view.contentY + (rowItem.height - height) / 2 : 0
        z: 2
    }
}
