import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"
import "../Patterns"

// A folder of a source that holds more than records: the folders under it,
// its records on the grid every record has, and its tracks on the list
// every track has.
Flickable {
    id: page

    property string source
    readonly property var about: JSON.parse(Sources.page(source, Library.remoteRev) || "{}")

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}
    SourcePlaylistEditor { id: playlistEditor }

    AlbumGridModel {
        id: cards
        kind: "source"
        key: page.source
        contentRev: Library.remoteRev + Library.remoteArtRev
        columns: grid.columns
        order: controls.sort
        group: controls.grouping
        filter: find.text
    }
    TrackListModel {
        id: tracks
        kind: "source"
        key: [page.source, controls.sort, controls.grouping, find.text].join("\u001f")
        filterRev: Library.remoteRev + Library.remoteArtRev
        playingPath: Player.path
    }

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16

        Text {
            text: page.about.title || page.about.name || ""
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        SourceTabs { model: page.about.tabs || [] }
        SourcePaging { about: page.about }
        Flow {
            width: parent.width
            spacing: Metrics.gap12
            SearchField { id: find; width: Metrics.screenSearch; placeholder: I18n.tr("Search", I18n.rev); onEscaped: text = "" }
            SourceLibraryControls { id: controls; source: page.source; entity: page.about.entity || "track" }
            PillButton { visible: page.about.playlist_edit === true; text: I18n.tr("New playlist", I18n.rev); onClicked: playlistEditor.begin({plugin:page.about.plugin}, "create") }
        }

        Flow {
            width: parent.width
            spacing: Metrics.gap8
            visible: (page.about.folders || []).length > 0

            Repeater {
                model: page.about.folders || []
                PillButton {
                    required property var modelData
                    glyph: Icons.folder
                    text: modelData.title
                    onClicked: Navigation.goKey("source", modelData.key)
                }
            }
        }

        AlbumGrid {
            id: grid
            width: parent.width
            height: implicitHeight
            visible: cards.albums > 0
            fitContent: true
            round: page.about.round === true
            roundIds: page.about.artist_cards || []
            model: cards
            onOpen: (albumId) => Navigation.go("album", albumId)
        }

        SectionHeader {
            width: parent.width
            visible: tracks.count > 0 && cards.albums > 0
            title: I18n.tr("Tracks", I18n.rev)
        }
        TrackList {
            width: parent.width
            height: implicitHeight
            visible: tracks.count > 0
            fitContent: true
            model: tracks
        }

        ShelfSkeleton {
            width: parent.width
            visible: cards.albums === 0 && tracks.count === 0 && page.about.loading === true
            tracks: page.about.entity === "track"
            round: page.about.round === true
            oneRow: false
            count: tracks ? Math.ceil(page.height / Metrics.rowRich)
                : CardLayout.columns(width) * Math.ceil(page.height / CardLayout.cardHeight)
        }
        SourceState {
            width: parent.width
            visible: cards.albums === 0 && tracks.count === 0 && (page.about.folders || []).length === 0 && page.about.loading !== true
            source: page.source
            about: page.about
        }
    }
}
