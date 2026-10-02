import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Components"
import "../Components"

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
    ScrollBar.vertical: ScrollBar {}

    AlbumGridModel {
        id: cards
        kind: "source"
        key: page.source
        contentRev: Library.remoteRev + Library.remoteArtRev
        columns: grid.columns
    }
    TrackListModel {
        id: tracks
        kind: "source"
        key: page.source
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

        SourceState {
            width: parent.width
            visible: cards.albums === 0 && tracks.count === 0 && (page.about.folders || []).length === 0
            source: page.source
            about: page.about
        }
    }
}
