import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Library (design/02 9, design/06): the filter columns in cascade and
// the records they leave, as cards or as rows. A fixed layout with no scroll of
// its own: only the columns and the result scroll.
Item {
    id: page

    // The filter columns turned on in the sources popover, kept.
    property var panes: initialPanes()
    function initialPanes() {
        const saved = SettingsModel.kept("ui.library.columns")
        if (saved) { try { const columns = JSON.parse(saved); if (Array.isArray(columns)) return columns } catch (_) {} }
        return (SettingsModel.kept("ui.library.panes") || "year,genre").split(",").filter(c => c !== "").concat(["artist", "album"])
    }
    function togglePane(column) {
        const on = page.panes.indexOf(column) < 0
        page.panes = on ? page.panes.concat([column]) : page.panes.filter(c => c !== column)
        SettingsModel.keep("ui.library.columns", JSON.stringify(page.panes))
        // A column turned off does not narrow what it no longer shows.
        if (!on) {
            Library.searchColumn(column, "")
            Library.choose(column, "", "")
        }
    }
    // The columns' height while their grip is dragged, or -1.
    property real dragged: -1
    // The columns' height when the grip was pressed.
    property real grabbed: 0
    readonly property bool panesShown: SettingsModel.panesOpen && page.panes.length > 0
    readonly property real panesHeight: !panesShown ? 0
        : dragged >= 0 ? dragged
        : SettingsModel.panesHeight > 0 ? SettingsModel.panesHeight : Metrics.panesHeight
    readonly property bool grid: SettingsModel.libraryView === "grid"
    property string group: SettingsModel.kept("ui.library.group")

    TrackListModel {
        id: tracks
        kind: "library"
        filterRev: Library.rev + Library.contentRev
        playingPath: Player.path
    }
    AlbumGridModel {
        id: records
        kind: "library"
        group: page.group
        contentRev: Library.rev + Library.contentRev
        columns: result.columns
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Library", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap12

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Library.summary
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            IconButton {
                id: sourceButton
                anchors.verticalCenter: parent.verticalCenter
                tip: I18n.tr("Sources and quality", I18n.rev)
                glyph: Icons.adjustmentsHorizontal
                round: true
                checked: Library.extraFiltered || sources.opened
                onClicked: sources.opened ? sources.close() : sources.open()

                SourcesPopover {
                    id: sources
                    showAlbumLibraries: true
                    showTrackLibraries: true
                    x: sourceButton.width - width
                    y: sourceButton.height + Metrics.gap6
                    panes: page.panes
                    onPaneToggled: (column) => page.togglePane(column)
                }
            }
            SortButton {
                value: Library.order
                options: [
                    { value: "artist", text: I18n.tr("Artist", I18n.rev) },
                    { value: "album", text: I18n.tr("Album", I18n.rev) },
                    { value: "year", text: I18n.tr("Year", I18n.rev) },
                    { value: "added", text: I18n.tr("Date added", I18n.rev), descending: true },
                    { value: "title", text: I18n.tr("Title", I18n.rev) },
                    { value: "plays", text: I18n.tr("Most played", I18n.rev), descending: true },
                    { value: "duration", text: I18n.tr("Duration", I18n.rev) }
                ]
                onPicked: (value) => Library.order = value
            }
            SortButton {
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.stack2
                directed: false
                value: page.group
                options: [
                    { value: "", text: I18n.tr("No grouping", I18n.rev) },
                    { value: "letter", text: I18n.tr("By letter", I18n.rev) },
                    { value: "artist", text: I18n.tr("By artist", I18n.rev) },
                    { value: "year", text: I18n.tr("By year", I18n.rev) },
                    { value: "decade", text: I18n.tr("By decade", I18n.rev) },
                    { value: "genre", text: I18n.tr("By genre", I18n.rev) },
                    { value: "label", text: I18n.tr("By label", I18n.rev) },
                    { value: "composer", text: I18n.tr("By composer", I18n.rev) },
                    { value: "quality", text: I18n.tr("By quality", I18n.rev) },
                    { value: "source", text: I18n.tr("By source", I18n.rev) },
                    { value: "added", text: I18n.tr("By date added", I18n.rev) },
                    { value: "type", text: I18n.tr("By release type", I18n.rev) }
                ]
                onPicked: (value) => { page.group = value; SettingsModel.keep("ui.library.group", value) }
            }
        }
    }

    LibraryCrumbs {
        id: crumbs
        anchors.top: head.bottom
        x: head.x
        width: head.width - toggle.width - Metrics.gap8
        height: Metrics.breadcrumb
    }
    IconButton {
        id: toggle
        anchors.right: head.right
        anchors.verticalCenter: crumbs.verticalCenter
        tip: SettingsModel.panesOpen ? I18n.tr("Hide the columns", I18n.rev) : I18n.tr("Show the columns", I18n.rev)
        glyph: Icons.columns3
        checked: SettingsModel.panesOpen
        onClicked: SettingsModel.panesOpen = !SettingsModel.panesOpen
    }

    LibraryPanes {
        id: panes
        anchors.top: crumbs.bottom
        x: head.x
        width: head.width
        height: page.panesHeight
        visible: page.panesShown
        shown: page.panes
    }

    ResizeHandle {
        id: grip
        visible: page.panesShown
        vertical: false
        anchors.top: panes.bottom
        anchors.horizontalCenter: panes.horizontalCenter
        width: head.width
        onStarted: page.grabbed = page.panesHeight
        onMoved: (delta) => page.dragged = Math.max(Metrics.panesMin, Math.min(Metrics.panesMax, page.grabbed + delta))
        onReleased: {
            if (page.dragged >= 0)
                SettingsModel.panesHeight = Math.round(page.dragged)
            page.dragged = -1
        }
    }

    LibraryToolbar {
        id: toolbar
        anchors.top: page.panesShown ? grip.bottom : crumbs.bottom
        anchors.topMargin: page.panesShown ? 0 : Metrics.gap12
        x: head.x
        width: head.width
        tracks: tracks
    }

    // The records the columns leave, as cards or as rows.
    AlbumGrid {
        id: result
        visible: records.albums > 0 || !Library.busy
        anchors.top: toolbar.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        bottomInset: Metrics.contentBottom
        list: !page.grid
        model: records
        onOpen: (albumId) => Navigation.go("album", albumId)
    }
    ShelfSkeleton {
        anchors.fill: result
        visible: Library.busy && records.albums === 0
        tracks: !page.grid
        oneRow: false
        count: Metrics.shelfLimit
    }
}
