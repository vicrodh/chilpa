import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "../Components"

// The record's cover in the metadata editor: the one it has or the one in
// wait, beside what to do with it -- a file, or candidates from the Cover Art
// Archive or Discogs, shown under it. A cover in wait is written with the
// next save. `doc` is the editor's view.
Column {
    id: root

    property var doc: ({})

    readonly property var cover: doc.cover || ({})
    readonly property var artwork: doc.artwork || ({})
    readonly property var candidates: artwork.candidates || []
    readonly property bool busy: artwork.searching === true || artwork.loading === true
    readonly property bool ready: doc.state === "ready" && !busy

    spacing: Metrics.gap8

    Row {
        width: parent.width
        spacing: Metrics.gap10

        ArtTile {
            width: Metrics.metadataCover
            height: width
            path: root.cover.path || ""
        }
        Column {
            width: parent.width - Metrics.metadataCover - parent.spacing
            spacing: Metrics.gap6
            Text {
                width: parent.width
                text: root.cover.waiting ? I18n.tr("New, saved with the next save", I18n.rev)
                      : root.cover.path ? I18n.tr("The record's cover", I18n.rev) : I18n.tr("No cover", I18n.rev)
                elide: Text.ElideRight
                font.pointSize: Type.meta
                font.weight: Type.semibold
                color: Theme.color.textPrimary
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
            Row {
                spacing: Metrics.gap6
                PillButton {
                    kind: "ghost"
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
    }

    // Where to look for another cover, under the one there is: the column is
    // too narrow to hold it beside the cover.
    Row {
        spacing: Metrics.gap6
        SegmentedControl {
            options: [{ text: "Cover Art Archive" }, { text: "Discogs" }]
            current: root.artwork.provider === "discogs" ? 1 : 0
            onActivated: (index) => MetadataEditor.setArtworkProvider(index === 1 ? "discogs" : "musicbrainz")
        }
        PillButton {
            kind: "ghost"
            glyph: Icons.search
            text: root.artwork.searching ? I18n.tr("Searching…", I18n.rev) : I18n.tr("Find covers", I18n.rev)
            enabled: root.ready
            onClicked: MetadataEditor.searchArtwork()
        }
    }

    Text {
        visible: root.busy
        text: root.artwork.loading ? I18n.tr("Fetching the cover…", I18n.rev) : I18n.tr("Searching…", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }

    Flow {
        width: root.width
        spacing: Metrics.gap8
        Repeater {
            model: root.candidates.length
            CoverCandidate {
                required property int index
                width: Metrics.metadataCandidate
                candidate: root.candidates[index]
                enabled: root.ready
                onChoose: MetadataEditor.chooseArtwork(index)
            }
        }
    }
}
