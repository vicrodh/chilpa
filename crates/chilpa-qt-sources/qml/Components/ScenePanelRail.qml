import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The rail is always A-Z, even when the grid's grouping is off.
Item {
    id: rail
    property var artists: []
    property string selectedId: ""
    signal artistSelected(string id)
    readonly property var entries: {
        var sorted = artists.slice().sort(function (a, b) { return String(a.title || a.name || "").localeCompare(String(b.title || b.name || "")) })
        var buckets = ({}), keys = []
        for (var i = 0; i < sorted.length; ++i) {
            var c = String(sorted[i].title || sorted[i].name || "").charAt(0).toUpperCase()
            var key = c >= "A" && c <= "Z" ? c : "#"
            if (buckets[key] === undefined) { buckets[key] = []; keys.push(key) }
            buckets[key].push(sorted[i])
        }
        keys.sort(function (a, b) { return a === "#" ? -1 : b === "#" ? 1 : a.localeCompare(b) })
        var out = []
        for (var g = 0; g < keys.length; ++g) {
            out.push({heading:keys[g]})
            for (var j = 0; j < buckets[keys[g]].length; ++j) out.push({artist:buckets[keys[g]][j]})
        }
        return out
    }
    ListView {
        anchors.fill: parent; anchors.leftMargin: Metrics.scenePadLeft
        model: rail.entries; clip: true; cacheBuffer: Metrics.scenePanelRow * 6
        boundsBehavior: Flickable.StopAtBounds
        delegate: Item {
            id: entry
            required property var modelData
            readonly property var artist: modelData.artist || ({})
            width: ListView.view.width - Metrics.gap12
            height: modelData.heading ? Metrics.scenePanelGroupHeader : Metrics.scenePanelRow
            Text {
                visible: !!entry.modelData.heading
                x: Metrics.gap8; anchors.bottom: parent.bottom; anchors.bottomMargin: Metrics.gap4
                text: entry.modelData.heading || ""; font.pointSize: Type.label
                font.weight: Type.semibold; color: Theme.color.textTertiary
            }
            Rectangle {
                id: line
                visible: !entry.modelData.heading; anchors.fill: parent
                radius: Metrics.radiusControl
                color: rail.selectedId === entry.artist.mbid ? Theme.color.rowSelected : hover.hovered ? Theme.color.stateHover : "transparent"
                ArtTile {
                    id: portrait
                    x: Metrics.gap8; anchors.verticalCenter: parent.verticalCenter
                    width: Metrics.scenePanelPortrait; height: width; radius: width / Metrics.gap2
                    readonly property var sourceRow: (entry.artist.sources || [])[0] || ({})
                    source: sourceRow.local ? sourceRow.image || "" : sourceRow.plugin && sourceRow.image ? Sources.art(sourceRow.plugin, sourceRow.image, Library.remoteArtRev) : ""
                }
                Icon { anchors.centerIn: portrait; visible: !portrait.ready; glyph: SourceGlyphs.musicArtist; iconFamily: SourceGlyphs.font; size: Metrics.iconXs; color: Theme.color.textTertiary }
                Text {
                    anchors.left: portrait.right; anchors.leftMargin: Metrics.gap12
                    anchors.right: parent.right; anchors.rightMargin: Metrics.gap8
                    anchors.verticalCenter: parent.verticalCenter
                    text: entry.artist.title || entry.artist.name || ""; textFormat: Text.PlainText
                    elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary
                }
                HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: rail.artistSelected(entry.artist.mbid) }
            }
        }
    }
}
