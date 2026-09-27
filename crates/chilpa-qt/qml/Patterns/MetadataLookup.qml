import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// Looking the record up: MusicBrainz or Discogs, with the title, artist and
// catalog number the form holds; the releases found; one loaded, how many
// of the record's tracks it matches, and applying it to the form. What a
// release does not say is left as it was. `doc` is the editor's view.
Column {
    id: root

    property var doc: ({})

    readonly property var lookup: doc.lookup || ({})
    readonly property var hits: lookup.hits || []
    readonly property var release: lookup.release || null
    readonly property bool busy: lookup.searching === true || lookup.loading === true
    property int chosen: -1

    spacing: Metrics.gap8

    Connections {
        target: MetadataEditor
        function onOpenChanged() { root.chosen = -1 }
    }

    Text {
        text: I18n.tr("Look up", I18n.rev)
        font.pointSize: Type.section
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }
    Row {
        spacing: Metrics.gap8
        SegmentedControl {
            options: [{ text: "MusicBrainz" }, { text: "Discogs" }]
            current: root.lookup.provider === "discogs" ? 1 : 0
            onActivated: (index) => MetadataEditor.setProvider(index === 1 ? "discogs" : "musicbrainz")
        }
        PillButton {
            glyph: Icons.search
            text: root.lookup.searching ? I18n.tr("Searching…", I18n.rev) : I18n.tr("Search", I18n.rev)
            enabled: !root.busy && root.doc.state === "ready"
            onClicked: {
                root.chosen = -1
                MetadataEditor.search()
            }
        }
    }

    Repeater {
        model: root.hits.length
        ReleaseHitRow {
            required property int index
            width: root.width
            rowIndex: index
            hit: root.hits[index]
            chosen: index === root.chosen
            onChoose: {
                root.chosen = index
                MetadataEditor.load(index)
            }
        }
    }

    Text {
        visible: root.lookup.loading === true
        text: I18n.tr("Loading the release…", I18n.rev)
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }

    Column {
        visible: root.release !== null && root.lookup.loading !== true
        width: root.width
        spacing: Metrics.gap6

        Text {
            width: parent.width
            text: root.release ? root.release.title + " — " + root.release.date : ""
            elide: Text.ElideRight
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: root.release ? I18n.trArgs("{} of {} tracks match this release", [root.lookup.matched, (root.doc.tracks || []).length], I18n.rev) : ""
            font.pointSize: Type.meta
            color: root.lookup.matched > 0 ? Theme.color.textSecondary : Theme.color.statusWarning
        }
        Row {
            spacing: Metrics.gap8
            PillButton {
                kind: "primary"
                text: I18n.tr("Apply to the form", I18n.rev)
                enabled: root.doc.state === "ready"
                onClicked: MetadataEditor.apply()
            }
            PillButton {
                kind: "ghost"
                glyph: Icons.arrowUpRight
                text: I18n.tr("Open its page", I18n.rev)
                visible: root.release !== null && root.release.url !== ""
                onClicked: Qt.openUrlExternally(root.release.url)
            }
        }
    }
}
