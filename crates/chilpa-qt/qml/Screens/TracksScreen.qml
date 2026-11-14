import "qrc:/qt/qml/app/chilpa/playlists/qml/Patterns"
import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/offline/qml/Components" as OfflineParts

// Tracks: every track of the library in one list, found by what is typed,
// in an order, and grouped or not. Grouped, its headers say the record, the
// artist or the letter, and a strip of letters jumps among them.
Item {
    id: page

    property string order: SettingsModel.kept("ui.tracks.order") || "default"
    property string group: SettingsModel.kept("ui.tracks.group") || "off"
    property string typed: Visit.recall("tracks.typed", "")
    property alias columnFilters: tableState.filters
    property alias shownColumns: tableState.columns
    TrackSheetState { id: tableState; settings: SettingsModel; visits: Visit }
    readonly property var scrollView: list.rows

    // Async incubation can yield between bindings; query only the completed page.
    Component.onCompleted: tracks.kind = "tracks"
    Component.onDestruction: {
        Visit.keep("tracks.typed", typed)
    }
    property bool selectionMode: false
    readonly property bool grouped: group !== "off"
    // Where each letter starts, read again each time the list arrives.
    property var letters: []

    TrackListModel {
        id: tracks
        key: [page.order === "default" ? "" : page.order, page.group, page.typed, "", JSON.stringify(tableState.effective)].join("\u001f")
        filterRev: Library.contentRev + Library.remoteRev + (tableState.marksFiltered ? Ratings.rev : 0)
        playingPath: Player.path
    }

    // Escape lets the selection go; Ctrl+A takes every track.
    Shortcut {
        enabled: !InputLayers.blocksShortcuts(page.Window.window) && (page.selectionMode)
        sequence: "Escape"
        onActivated: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }
    Shortcut {
        enabled: !InputLayers.blocksShortcuts(page.Window.window)
        sequence: StandardKey.SelectAll
        onActivated: {
            page.selectionMode = true
            tracks.selectAll()
        }
    }

    TrackSheetTools {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        count: tracks.summary
        typed: page.typed
        order: page.order
        group: page.group
        shownColumns: page.shownColumns
        selectionMode: page.selectionMode
        onSearched: text => page.typed = text
        onOrdered: value => { page.order = value; SettingsModel.keep("ui.tracks.order", value) }
        onGrouped: value => { page.group = value; SettingsModel.keep("ui.tracks.group", value) }
        onColumned: columns => { columnFilter.close(); tableState.keepColumns(columns) }
        onSelectionToggled: { page.selectionMode = !page.selectionMode; tracks.clearSelection() }
    }

    readonly property var sourceNames: {
        const names = Object.assign({}, tableState.sourceNames)
        for (let i = 0; i < Library.sourceKeys.length; ++i) names[Library.sourceKeys[i]] = Library.sourceLabels[i]
        return names
    }
    TrackFilterTrail {
        id: trail
        objectName: "tracksFilterTrail"
        x: head.x; width: head.width
        anchors.top: head.bottom
        anchors.topMargin: visible ? Metrics.gap10 : 0
        filters: tableState.effective
        search: page.typed
        sourceNames: page.sourceNames
        onEdited: (field, label, anchor) => {
            if (field === "search") head.focusSearch()
            else columnFilter.show(field, label, page.columnFilters[field], anchor)
        }
        onRemoved: field => { if (field === "search") { page.typed = ""; head.clearSearch() } else tableState.apply(field, null) }
        onCleared: { columnFilter.close(); page.columnFilters = ({}); page.typed = ""; head.clearSearch(); tableState.sourceNames = ({}) }
    }

    // The group the first row shown is in, above the rows: bands between
    // the rows made the view misjudge far rows, and the first scroll after
    // a far jump leapt hundreds of rows. Each group's first row carries a
    // divider instead.
    readonly property string currentGroup: grouped ? list.rows.currentSection : ""
    Text {
        id: groupName
        objectName: "tracksGroupName"
        anchors.top: trail.bottom
        anchors.topMargin: Metrics.gap12
        x: head.x
        width: sheet.width
        height: page.grouped ? Metrics.rowGroup : 0
        visible: page.grouped
        leftPadding: Metrics.gap2
        text: page.currentGroup
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
        font.pointSize: Type.meta
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }

    // How the tracks last made available offline from a list go, in the gap over it.
    OfflineParts.OfflineLine { objectName: "tracksOfflineLine"; x: head.x; width: sheet.width; anchors.bottom: groupName.top; anchors.bottomMargin: Metrics.gap6; kind: "tracks"; rule: false }
    Flickable {
        id: sheet
        objectName: "tracksSheet"
        anchors.top: groupName.bottom
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap8
        x: head.x
        width: head.width - Metrics.gap12 - (strip.visible ? strip.width + Metrics.gap8 : 0)
        contentWidth: Math.max(width, list.columns.minimumWidth)
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Controls.ScrollBar.horizontal: ViewportScrollBar { objectName: "tracksHorizontalScrollbar"; policy: Controls.ScrollBar.AsNeeded; gutter: -Metrics.gap4; bottomClearance: Metrics.floatingPlayerClearance; z: 2 }
        TrackSheetHeader {
            id: sheetHead
            width: sheet.contentWidth
            cols: list.columns
            filters: tableState.effective
            onRequested: (field, label, anchor) => columnFilter.show(field, label, page.columnFilters[field], anchor)
        }
        TrackList {
            id: list
            y: sheetHead.height
            width: sheet.contentWidth
            height: Math.max(0, sheet.height - y)
            scrollGutter: Metrics.gap8
            scrollbarOverlay: page
            scrollbarEdge: sheet.contentX + sheet.width
            visibleLeft: sheet.contentX
            visibleRight: sheet.contentX + sheet.width
            bottomInset: Metrics.contentBottom
            header: false
            sheet: true
            optionalColumns: page.shownColumns
            rich: SettingsModel.density === "rich"
            showAlbum: true
            showArtist: true
            showPlays: true
            selectionMode: page.selectionMode
            model: tracks
        }
    }
    TrackColumnChoices { id: choices; key: tracks.key; filterRev: tracks.filterRev }
    TrackColumnFilter {
        id: columnFilter
        model: choices
        contentViewport: page
        sourceNames: page.sourceNames
        onApplied: (field, values, labels) => tableState.apply(field, values, labels)
    }
    Connections {
        target: tracks
        function onModelReset() {
            page.letters = JSON.parse(tracks.letters(0) || "[]")
        }
    }
    Binding {
        target: list.rows.section
        property: "property"
        value: page.grouped ? "group" : ""
    }

    AlphaStrip {
        id: strip
        objectName: "tracksLetters"
        visible: page.grouped && tracks.count > 0 && page.letters.length > 0
        anchors.top: sheet.top
        anchors.bottom: sheet.bottom
        anchors.bottomMargin: Metrics.contentBottom
        anchors.right: head.right
        letters: page.letters
        onJumped: (row) => list.jumpTo(row)
    }

    Loader {
        objectName: "tracksLoadingSkeleton"
        x: sheet.x; y: sheet.y + sheetHead.height
        width: sheet.width; height: list.height
        active: tracks.count === 0 && (!tracks.ready || Library.busy)
        sourceComponent: ShelfSkeleton {
            tracks: true
            headerHeight: 0
            trackRowHeight: list.rich ? Metrics.rowRich : Metrics.rowCompact
            count: Math.min(Metrics.trackSkeletonMaxRows, Math.ceil(list.height / trackRowHeight))
        }
    }
    Text {
        objectName: "tracksEmptyState"
        visible: tracks.ready && !Library.busy && tracks.count === 0
        anchors.centerIn: sheet
        text: page.typed !== "" || tableState.filtered ? I18n.tr("No tracks match your search.", I18n.rev) : I18n.tr("No tracks in the selected libraries yet.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }

    PlaylistSelectionBar {
        visible: page.selectionMode && tracks.selectedCount > 0
        z: 2
        model: tracks
        playlistName: I18n.tr("Tracks", I18n.rev)
        canRemove: false
        x: (page.width - width) / 2
        y: page.height - height - Metrics.selectionBottom
        onFinished: {
            tracks.clearSelection()
            page.selectionMode = false
        }
    }
}
