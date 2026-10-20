import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/SectionChoices.js" as SectionChoices
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "../Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"

// I compose Home from local and source modules in the chosen order and size.
ListView {
    id: page
    property int homeRefresh: 0
    readonly property alias homeSections: remote.sections
    onVisibleChanged: if (visible) homeRefresh++
    property bool presented: false
    property bool leaving: false
    Connections {
        target: page.Window.window
        function onFrameSwapped() { if (page.visible) page.presented = true }
    }

    readonly property string preference: "ui.home.sections"
    property var kept: []
    function restore() { try { kept = JSON.parse(SettingsModel.kept(preference) || "[]") } catch (_) { kept = [] } }
    Component.onCompleted: restore()
    readonly property var available: [
        { id: "library.recentlyPlayed", title: I18n.tr("Recently played", I18n.rev), kind: "albums", source: "recentlyPlayed" },
        { id: "everywhere.recentlyPlayed", title: I18n.tr("Recently played", I18n.rev), configTitle: I18n.tr("Recently played, all sources", I18n.rev), kind: "albums", source: "recentlyPlayedEverywhere" },
        { id: "library.recentPlaylists", title: I18n.tr("Recently listened playlists", I18n.rev), kind: "playlists", allSources: false },
        { id: "everywhere.recentPlaylists", title: I18n.tr("Recently listened playlists", I18n.rev), configTitle: I18n.tr("Recently listened playlists, all sources", I18n.rev), kind: "playlists", allSources: true },
        { id: "library.recentlyAdded", title: I18n.tr("Recently added", I18n.rev), kind: "albums", source: "recentlyAdded" },
        { id: "library.mostPlayed", title: I18n.tr("Most played", I18n.rev), kind: "tracks" }
    ].concat(remote.sections.map(section => Object.assign({}, section, { id: "source:" + section.homeId, entityKind: section.kind, kind: "shelf",
        previous_ids: (section.previous_ids || []).map(id => "source:" + section.sourceId + ":" + id) })))
    // The kept choice over what there is today: a shelf that went is left
    // out, and a new one comes last, shown.
    readonly property var sections: SectionChoices.order(available, kept)
    // I update rows by id so a late source shelf does not remount the Home
    // shelves already on screen. Replacing a JS array model discarded them.
    KeyedModel {
        id: visibleSections
        items: page.sections.filter(section => section.enabled !== false
            && (section.kind !== "shelf" || (section.items || []).length > 0))
    }
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

    model: visibleSections
    clip: true
    spacing: Metrics.gap8
    cacheBuffer: height * 1.25 // derived: the viewport's runway, as the reference Home uses
    topMargin: Metrics.contentTop
    bottomMargin: Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    // I keep one row when a chosen count makes the shelf wider than its viewport.
    component AlbumShelf: AlbumCarousel {
        id: shelf
        viewport: page
        property var entry: ({})
        readonly property bool ready: records.ready
        readonly property bool shown: records.albums > 0
        title: entry.title || ""
        size: entry.size || 0
        model: records
        spacing: Metrics.gap8
        AlbumGridModel { id: records; kind: shelf.entry.source || ""; contentRev: Library.contentRev + (kind === "recentlyAdded" ? 0 : Library.listeningRev + Library.remoteRev + Library.remoteArtRev); limit: shelf.entry.size || Metrics.shelfLimit }
    }
    component TrackShelf: TrackCarousel {
        id: tracks
        viewport: page
        property var entry: ({})
        readonly property bool ready: most.ready
        readonly property bool shown: most.count > 0
        title: entry.title || ""
        showPlays: true
        model: most
        TrackListModel { id: most; kind: "mostPlayed"; filterRev: Library.contentRev + Library.listeningRev; limit: tracks.entry.size || CardLayout.columns(tracks.width); playingPath: Player.path }
    }
    component PlaylistShelf: HomePlaylistShelf {}
    // A source's shelf, cut to the size chosen.
    component SourceRail: SourceShelf {
        viewport: page
        property var entry: ({})
        oneRow: true
        section: Object.assign({}, entry, { kind: entry.entityKind || "album", items: (entry.items || []).slice(0, entry.size || undefined) })
    }

    header: Item {
        width: page.width
        height: column.height + Metrics.gap8
        Column {
            id: column
            x: Metrics.contentX
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
                    onClicked: { customization.active = true; customization.item.open() }
                }
            }

            Text {
                visible: remote.ready && LibraryFolders.count === 0 && remote.sections.length === 0
                topPadding: Metrics.section
                text: I18n.tr("Nothing here yet: add a folder to the library and its records show up here.", I18n.rev)
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
            PillButton {
                visible: remote.ready && LibraryFolders.count === 0 && remote.sections.length === 0
                kind: "primary"
                glyph: Icons.folderPlus
                text: I18n.tr("Add a folder", I18n.rev)
                onClicked: Navigation.go("libraryFolders", 0)
            }

        }
    }
    delegate: Item {
        required property var entry
        required property string key
        objectName: "homeShelf"
        width: page.width
        height: module.height
        HomeSection {
            id: module
            x: Metrics.contentX
            width: page.width - Metrics.contentX * 2 // derived: padding on both sides
            entry: parent.entry
            started: !page.leaving && (entry.kind !== "shelf" || page.presented)
            content: entry.kind === "albums" ? albumShelf : entry.kind === "playlists" ? playlistShelf : entry.kind === "tracks" ? trackShelf : sourceRail
        }
    }
    SourceHomeSections { id: remote; active: page.presented; refresh: page.homeRefresh; rendered: false }
    Component { id: albumShelf; AlbumShelf { entry: parent.entry; width: parent ? parent.width : 0 } }
    Component { id: trackShelf; TrackShelf { entry: parent.entry; width: parent ? parent.width : 0 } }
    Component { id: playlistShelf; PlaylistShelf { entry: parent.entry; width: parent ? parent.width : 0 } }
    Component { id: sourceRail; SourceRail { entry: parent.entry; width: parent ? parent.width : 0 } }

    Loader {
        id: customization
        active: false
        sourceComponent: SectionsCustomizer {
            objectName: "homeCustomizer"
            grouped: true
            sections: page.sections
            groupSources: remote.sources
            onToggled: (index, on) => page.change(index, "enabled", on)
            onSized: (index, size) => page.change(index, "size", size)
            onMoved: (index, delta) => page.move(index, delta)
            onRestored: page.keep([])
        }
    }
}
