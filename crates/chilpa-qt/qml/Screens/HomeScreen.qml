import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// Home (design/06): what played last, here and everywhere, what came in
// last, the most played tracks, and the sources' own shelves after them.
// Which show, in what order and how many each has, the listener chooses in
// Customize, the same choice a source's page offers (the owner, 2026-10-12).
Flickable {
    id: page
    property int homeRefresh: 0
    readonly property alias homeSections: remote.sections
    onVisibleChanged: if (visible) homeRefresh++

    readonly property string preference: "ui.home.sections"
    property var kept: []
    function restore() { try { kept = JSON.parse(SettingsModel.kept(preference) || "[]") } catch (_) { kept = [] } }
    Component.onCompleted: restore()
    readonly property var available: [
        { id: "library.recentlyPlayed", title: I18n.tr("Recently played", I18n.rev), kind: "albums", source: "recentlyPlayed" },
        { id: "everywhere.recentlyPlayed", title: I18n.tr("Recently played, all sources", I18n.rev), kind: "albums", source: "recentlyPlayedEverywhere" },
        { id: "library.recentlyAdded", title: I18n.tr("Recently added", I18n.rev), kind: "albums", source: "recentlyAdded" },
        { id: "library.mostPlayed", title: I18n.tr("Most played", I18n.rev), kind: "tracks" }
    ].concat(remote.sections.map(section => Object.assign({}, section, { id: "source:" + section.homeId, kind: "shelf" })))
    // The kept choice over what there is today: a shelf that went is left
    // out, and a new one comes last, shown.
    readonly property var sections: kept.map(choice => Object.assign({}, available.find(s => s.id === choice.id) || {}, choice))
        .filter(choice => available.some(s => s.id === choice.id))
        .concat(available.filter(s => !kept.some(choice => choice.id === s.id)))
    function change(index, field, value) {
        const rows = sections.map(s => ({ id: s.id, enabled: s.enabled !== false, size: s.size || 0 }))
        rows[index][field] = value
        keep(rows)
    }
    function move(index, delta) {
        const rows = sections.map(s => ({ id: s.id, enabled: s.enabled !== false, size: s.size || 0 }))
        const other = index + delta
        if (other < 0 || other >= rows.length)
            return
        const row = rows[index]; rows[index] = rows[other]; rows[other] = row
        keep(rows)
    }
    function keep(rows) { kept = rows; SettingsModel.keep(preference, JSON.stringify(rows)) }

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    // A shelf of records: one row unless a size was chosen.
    component AlbumShelf: Column {
        id: shelf
        property var entry: ({})
        spacing: Metrics.gap8
        visible: records.albums > 0
        AlbumGridModel { id: records; kind: shelf.entry.source || ""; contentRev: Library.contentRev; limit: shelf.entry.size || Metrics.shelfLimit; maxRows: shelf.entry.size ? 0 : 1; columns: grid.columns }
        SectionHeader { width: parent.width; title: shelf.entry.title || "" }
        AlbumGrid { id: grid; width: parent.width; height: implicitHeight; fitContent: true; model: records; onOpen: (albumId) => Navigation.go("album", albumId) }
    }
    component TrackShelf: Column {
        id: tracks
        property var entry: ({})
        spacing: Metrics.gap8
        visible: most.count > 0
        TrackListModel { id: most; kind: "mostPlayed"; filterRev: Library.contentRev; limit: tracks.entry.size || Metrics.mostPlayedLimit; playingPath: Player.path }
        SectionHeader { width: parent.width; title: tracks.entry.title || "" }
        TrackList { width: parent.width; height: implicitHeight; fitContent: true; showPlays: true; model: most }
    }
    // A source's shelf, cut to the size chosen.
    component SourceRail: SourceShelf {
        property var entry: ({})
        section: Object.assign({}, entry, { items: (entry.items || []).slice(0, entry.size || undefined) })
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap8

        Item {
            width: parent.width
            height: homeTitle.height
            Text {
                id: homeTitle
                text: I18n.tr("Home", I18n.rev)
                font.pointSize: Type.title
                font.weight: Type.bold
                font.letterSpacing: Type.titleTracking
                color: Theme.color.textPrimary
            }
            IconButton {
                objectName: "homeCustomize"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                size: Metrics.button32
                round: true
                glyph: Icons.settings
                tip: I18n.tr("Customize", I18n.rev)
                onClicked: customizer.open()
            }
        }

        Text {
            visible: LibraryFolders.count === 0 && remote.sections.length === 0
            topPadding: Metrics.section
            text: I18n.tr("Nothing here yet: add a folder to the library and its records show up here.", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        PillButton {
            visible: LibraryFolders.count === 0 && remote.sections.length === 0
            kind: "primary"
            glyph: Icons.folderPlus
            text: I18n.tr("Add a folder", I18n.rev)
            onClicked: Navigation.go("libraryFolders", 0)
        }

        Repeater {
            model: page.sections.filter(section => section.enabled !== false)
            Loader {
                required property var modelData
                objectName: "homeShelf"
                width: column.width
                visible: status === Loader.Ready && item.visible
                sourceComponent: modelData.kind === "albums" ? albumShelf : modelData.kind === "tracks" ? trackShelf : sourceRail
                onLoaded: item.entry = Qt.binding(() => modelData)
            }
        }
        SourceHomeSections { id: remote; width: parent.width; refresh: page.homeRefresh; rendered: false }
    }
    Component { id: albumShelf; AlbumShelf { width: parent ? parent.width : 0 } }
    Component { id: trackShelf; TrackShelf { width: parent ? parent.width : 0 } }
    Component { id: sourceRail; SourceRail { width: parent ? parent.width : 0 } }

    SectionsCustomizer {
        id: customizer
        sections: page.sections
        sizes: [{ value: "0", text: I18n.tr("One row", I18n.rev) }, { value: "6", text: "6" }, { value: "12", text: "12" }, { value: "18", text: "18" }, { value: "24", text: "24" }]
        defaultSize: 0
        onToggled: (index, on) => page.change(index, "enabled", on)
        onSized: (index, size) => page.change(index, "size", size)
        onMoved: (index, delta) => page.move(index, delta)
        onRestored: page.keep([])
    }
}
