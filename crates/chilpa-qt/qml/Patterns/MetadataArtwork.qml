import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The record's cover in the metadata editor: the one it has or the one in
// wait, its size and where it came from; choosing one from a file, or
// looking for candidates in MusicBrainz's cover archive or on Discogs. A
// cover in wait is written with the next save. `doc` is the editor's view.
Column {
    id: root

    property var doc: ({})

    readonly property var cover: doc.cover || ({})
    readonly property var artwork: doc.artwork || ({})
    readonly property var candidates: artwork.candidates || []
    readonly property bool busy: artwork.searching === true || artwork.loading === true
    readonly property bool lookups: doc.state === "ready" && !busy

    spacing: Metrics.gap8

    Text {
        text: I18n.tr("Cover", I18n.rev)
        font.pointSize: Type.section
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }

    Row {
        spacing: Metrics.gap12
        ArtTile {
            width: Metrics.metadataCover
            height: width
            path: root.cover.path || ""
        }
        Column {
            spacing: Metrics.gap6
            Text {
                text: root.cover.waiting ? I18n.tr("New, saved with the next save", I18n.rev)
                      : root.cover.path ? I18n.tr("The record's cover", I18n.rev) : I18n.tr("No cover", I18n.rev)
                font.pointSize: Type.meta
                font.weight: Type.medium
                color: root.cover.waiting ? Theme.color.textPrimary : Theme.color.textSecondary
            }
            Text {
                visible: root.cover.width > 0
                text: root.cover.width + " × " + root.cover.height
                      + (root.cover.from === "file" ? " · " + I18n.tr("from a file", I18n.rev)
                         : root.cover.from === "musicbrainz" ? " · Cover Art Archive"
                         : root.cover.from === "discogs" ? " · Discogs" : "")
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            PillButton {
                glyph: Icons.folderOpen
                text: I18n.tr("Choose a file…", I18n.rev)
                enabled: root.doc.state === "ready" && root.artwork.loading !== true
                onClicked: MetadataEditor.artworkFromFile()
            }
            PillButton {
                kind: "ghost"
                visible: root.cover.waiting === true
                text: I18n.tr("Keep the current one", I18n.rev)
                onClicked: MetadataEditor.clearArtwork()
            }
        }
    }

    Row {
        spacing: Metrics.gap8
        SegmentedControl {
            options: [{ text: "Cover Art Archive" }, { text: "Discogs" }]
            current: root.artwork.provider === "discogs" ? 1 : 0
            onActivated: (index) => MetadataEditor.setArtworkProvider(index === 1 ? "discogs" : "musicbrainz")
        }
        PillButton {
            glyph: Icons.search
            text: root.artwork.searching ? I18n.tr("Searching…", I18n.rev) : I18n.tr("Find covers", I18n.rev)
            enabled: root.lookups
            onClicked: MetadataEditor.searchArtwork()
        }
    }

    Text {
        visible: root.artwork.loading === true
        text: I18n.tr("Fetching the cover…", I18n.rev)
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }

    Flow {
        width: root.width
        spacing: Metrics.gap12
        Repeater {
            model: root.candidates.length
            CoverCandidate {
                required property int index
                width: Metrics.metadataCandidate
                candidate: root.candidates[index]
                enabled: root.lookups
                onChoose: MetadataEditor.chooseArtwork(index)
            }
        }
    }
}
