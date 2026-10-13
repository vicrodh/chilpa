import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The release column. The page's album shelves become a single
// windowed list of section headers and card rows, so large catalogs stay cheap.
Item {
    id: pane
    property var artist: ({})
    property bool includeSources: false
    property var source: ({})
    property var document: ({})
    signal artistOpened()
    readonly property int columns: CardLayout.columns(width - Metrics.scenePanelPadTotal)
    readonly property real trackWidth: CardLayout.cardWidth
    readonly property var sections: (document.shelves || []).filter(function (s) { return s.kind === "album" && s.role !== "latest" && s.id !== "latest" && (s.items || []).length > 0 })
    readonly property var entries: {
        var out = []
        for (var i = 0; i < sections.length; ++i) {
            var section = sections[i], albums = section.items || []
            out.push({id: section.id + ":heading", heading: section.title || "", count: albums.length})
            for (var j = 0; j < albums.length; j += columns)
                out.push({id: section.id + ":" + albums[j].ref, albums:albums.slice(j, j + columns)})
            if (section.key && (section.more || section.loading || section.error))
                out.push({id: section.id + ":more", continuation:section})
        }
        return out
    }
    readonly property bool idle: !artist.mbid
    readonly property bool noSource: !!artist.mbid && !source.plugin
    readonly property bool loading: !idle && !noSource && document.loading === true && entries.length === 0
    readonly property bool failed: !idle && !noSource && !!document.error && entries.length === 0
    readonly property bool empty: !idle && !noSource && !loading && !failed && entries.length === 0
    KeyedModel { id: albumRows; items: pane.entries }

    Column {
        visible: pane.idle || pane.noSource || pane.loading || pane.failed || pane.empty
        anchors.centerIn: parent; width: Math.min(parent.width - Metrics.scenePanelPadTotal, Metrics.sceneErrorWidth)
        spacing: Metrics.gap12
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            glyph: pane.empty ? Icons.disc : SourceGlyphs.musicArtist
            iconFamily: pane.empty ? "" : SourceGlyphs.font
            size: Metrics.iconPlay; color: Theme.color.textTertiary
        }
        Text {
            width: parent.width; textFormat: Text.PlainText; wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter; font.pointSize: Type.body; color: Theme.color.textSecondary
            text: pane.idle ? I18n.tr("Select an artist to see their albums", I18n.rev)
                : pane.noSource ? (pane.includeSources ? I18n.tr("No plugin catalog match for this artist", I18n.rev)
                    : I18n.tr("Plugin results were not included for this scene", I18n.rev))
                : pane.loading ? I18n.tr("Loading albums…", I18n.rev)
                : pane.failed ? I18n.tr("Failed to load albums", I18n.rev)
                : I18n.tr("No albums found", I18n.rev)
        }
        Text {
            visible: pane.failed; width: parent.width
            text: pane.document.error || ""; textFormat: Text.PlainText; wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter; font.pointSize: Type.meta; color: Theme.color.textTertiary
        }
        PillButton {
            visible: pane.noSource || pane.empty || pane.failed
            anchors.horizontalCenter: parent.horizontalCenter
            text: I18n.tr("Open artist", I18n.rev)
            onClicked: pane.artistOpened()
        }
    }
    ListView {
        id: list
        visible: pane.entries.length > 0
        x: Metrics.scenePanelPadLeft; width: Math.max(0, parent.width - Metrics.scenePanelPadTotal); height: parent.height
        model: albumRows; clip: true; cacheBuffer: CardLayout.cardHeight * 2
        boundsBehavior: Flickable.StopAtBounds
        delegate: Item {
            id: row
            required property var entry
            readonly property var modelData: entry
            readonly property bool heading: modelData.heading !== undefined
            readonly property bool continuation: !!modelData.continuation
            width: list.width
            height: heading ? Metrics.scenePanelSectionHeader
                : continuation ? (row.modelData.continuation.error ? Metrics.sceneFooterHeight : Metrics.sceneNavHeight)
                : CardLayout.cardHeight + Metrics.gridGap
            Text {
                id: sectionTitle
                visible: row.heading; text: row.modelData.heading || ""
                font.pointSize: Type.titleSmall; font.weight: Type.semibold; color: Theme.color.textPrimary
            }
            Text {
                visible: row.heading; anchors.left: sectionTitle.right; anchors.leftMargin: Metrics.gap12
                anchors.baseline: sectionTitle.baseline; text: row.modelData.count || ""
                font.pointSize: Type.meta; color: Theme.color.textTertiary
            }
            Rectangle {
                visible: row.heading; y: Metrics.scenePanelSectionHeader - Metrics.gap16
                width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline
            }
            Repeater {
                model: row.heading || row.continuation ? [] : row.modelData.albums || []
                SceneAlbumCard {
                    required property var modelData
                    required property int index
                    x: index * (pane.trackWidth + Metrics.gridGap)
                    album: modelData; plugin: pane.source.plugin || ""
                }
            }
            Column {
                visible: row.continuation; spacing: Metrics.gap8
                Text {
                    visible: !!row.modelData.continuation?.error
                    width: list.width; text: row.modelData.continuation?.error || ""
                    textFormat: Text.PlainText; wrapMode: Text.WordWrap
                    font.pointSize: Type.meta; color: Theme.color.statusNegative
                }
                PillButton {
                    objectName: row.continuation ? "sceneSectionMore" : ""
                    visible: !!row.modelData.continuation?.more || row.modelData.continuation?.loading === true
                    enabled: row.modelData.continuation?.loading !== true
                    text: row.modelData.continuation?.loading === true ? I18n.tr("Loading…", I18n.rev) : I18n.tr("Load more", I18n.rev)
                    onClicked: Sources.command("section_more", row.modelData.continuation.key, "")
                }
            }
        }
    }
}
