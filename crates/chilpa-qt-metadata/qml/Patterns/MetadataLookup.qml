import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.metadata
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// Looking the record up: MusicBrainz or Discogs, with the title, artist and
// catalog number the form holds; the releases found, a few rows at a time;
// one loaded, how many of the record's tracks it matches, and applying it to
// the form. What a release does not say is left as it was. `doc` is the
// editor's view.
Column {
    id: root

    property var doc: ({})

    readonly property var lookup: doc.lookup || ({})
    readonly property var hits: lookup.hits || []
    readonly property var release: lookup.release || null
    readonly property int selectedDisc: lookup.selected_disc || 0
    readonly property bool busy: lookup.searching === true || lookup.loading === true
    property int chosen: -1

    spacing: Metrics.gap8

    Connections {
        target: MetadataEditor
        function onOpenChanged() { root.chosen = -1 }
    }

    Row {
        width: parent.width
        spacing: Metrics.gap8
        SegmentedControl {
            id: provider
            options: [{ text: "MusicBrainz" }, { text: "Discogs" }]
            current: root.lookup.provider === "discogs" ? 1 : 0
            onActivated: (index) => MetadataEditor.setProvider(index === 1 ? "discogs" : "musicbrainz")
        }
        PillButton {
            glyph: Icons.search
            text: root.lookup.searching ? I18n.tr("Searching…", I18n.rev) : I18n.tr("Look up", I18n.rev)
            enabled: !root.busy && root.doc.state === "ready"
            onClicked: {
                root.chosen = -1
                root.forceActiveFocus()
                MetadataEditor.search()
            }
        }
    }

    ListView {
        width: parent.width
        height: Math.min(count, Metrics.metadataHitsShown) * Metrics.metadataHit
        visible: count > 0
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
        model: root.hits.length
        delegate: ReleaseHitRow {
            required property int index
            width: ListView.view.width
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
        width: parent.width
        spacing: Metrics.gap6

        Row {
            visible: root.release !== null && root.release.discs > 1
            spacing: Metrics.gap8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Discs", I18n.rev)
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            SelectField {
                id: mediumSelect
                options: {
                    if (!root.release) return []
                    const choices = [I18n.tr("All discs", I18n.rev)]
                    for (let disc = 1; disc <= root.release.discs; ++disc) {
                        const catalog = (root.release.catalog_numbers || [])[disc - 1] || ""
                        choices.push(I18n.trArgs("Disc {}", [disc], I18n.rev) + (catalog ? " · " + catalog : ""))
                    }
                    return choices
                }
                values: mediumSelect.options.map((_, index) => String(index))
                details: root.release ? [""].concat(root.release.media_titles || []) : []
                value: String(root.selectedDisc)
                onPicked: (value) => MetadataEditor.selectDisc(Number(value))
            }
        }

        Text {
            width: parent.width
            text: root.release ? root.release.title + " — " + root.release.date + " · "
                + (root.selectedDisc > 0
                    ? I18n.trArgs("{} tracks match disc {}", [root.lookup.matched, root.selectedDisc], I18n.rev)
                    : I18n.trArgs("{} of {} tracks match this release", [root.lookup.matched, (root.doc.tracks || []).length], I18n.rev)) : ""
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: root.lookup.matched > 0 ? Theme.color.textSecondary : Theme.color.statusWarning
        }
        Row {
            spacing: Metrics.gap8
            PillButton {
                kind: "primary"
                text: I18n.tr("Apply to the form", I18n.rev)
                enabled: root.doc.state === "ready" && root.lookup.matched > 0
                onClicked: {
                    // The field being typed in lets go, and shows what the release gave.
                    root.forceActiveFocus()
                    MetadataEditor.apply()
                }
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
