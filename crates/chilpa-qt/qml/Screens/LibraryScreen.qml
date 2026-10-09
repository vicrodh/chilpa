import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Library (design/02 9, design/06): the filter columns in cascade and
// the records they leave, as cards or as rows. A fixed layout with no scroll of
// its own: only the columns and the result scroll.
Item {
    id: page

    // The filter columns turned on in the sources popover, kept.
    property var panes: (SettingsModel.kept("ui.library.panes") || "year,genre").split(",").filter(c => c !== "")
    function togglePane(column) {
        const on = page.panes.indexOf(column) < 0
        page.panes = on ? page.panes.concat([column]) : page.panes.filter(c => c !== column)
        SettingsModel.keep("ui.library.panes", page.panes.join(","))
        // A column turned off does not narrow what it no longer shows.
        if (!on)
            Library.choose(column, "", "")
    }
    // The columns' height while their grip is dragged, or -1.
    property real dragged: -1
    // The columns' height when the grip was pressed.
    property real grabbed: 0
    readonly property real panesHeight: !SettingsModel.panesOpen ? 0
        : dragged >= 0 ? dragged
        : SettingsModel.panesHeight > 0 ? SettingsModel.panesHeight : Metrics.panesHeight
    readonly property bool grid: SettingsModel.libraryView === "grid"

    TrackListModel {
        id: tracks
        kind: "library"
        filterRev: Library.rev
        playingPath: Player.path
    }
    AlbumGridModel {
        id: records
        kind: "library"
        contentRev: Library.rev
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
                    { value: "added", text: I18n.tr("Date added", I18n.rev) },
                    { value: "title", text: I18n.tr("Title", I18n.rev) },
                    { value: "plays", text: I18n.tr("Most played", I18n.rev) },
                    { value: "duration", text: I18n.tr("Duration", I18n.rev) }
                ]
                onPicked: (value) => Library.order = value
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
        visible: SettingsModel.panesOpen
        shown: page.panes
    }

    ResizeHandle {
        id: grip
        visible: SettingsModel.panesOpen
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
        anchors.top: SettingsModel.panesOpen ? grip.bottom : crumbs.bottom
        anchors.topMargin: SettingsModel.panesOpen ? 0 : Metrics.gap12
        x: head.x
        width: head.width
        tracks: tracks
    }

    // The records the columns leave, as cards or as rows.
    AlbumGrid {
        id: result
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
}
