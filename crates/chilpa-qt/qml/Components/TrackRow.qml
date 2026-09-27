// hot-path: one per visible track, recycled while the list scrolls.
import QtQuick
import "../Theme"
import "../Primitives"

// A track, or a disc header, in either density (design/03 TrackRow).
// It reads its roles and the shared column widths; it has no mouse handling
// of its own: the list resolves hover and clicks for every row at once.
Item {
    id: row

    required property int index
    required property int rowKind
    required property int number
    required property string title
    required property string artist
    required property string album
    required property string duration
    required property int plays
    required property string quality
    required property int tier
    required property bool playing
    required property bool selected
    required property string headerLabel
    required property string headerMeta
    required property url art

    property TrackColumns cols
    property bool hovered: false

    readonly property bool header: rowKind === 1
    readonly property bool rich: cols.rich

    width: cols.width
    height: header ? Metrics.rowGroup : rich ? Metrics.rowRich : Metrics.rowCompact

    ListRowSurface {
        anchors.fill: parent
        index: row.index
        radius: row.rich ? Metrics.radiusCard : Metrics.radiusRow
        visible: !row.header
        hovered: row.hovered
        selected: row.selected
        emphasized: row.playing && row.rich
    }

    Row {
        visible: row.header
        x: row.cols.indexX
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Icon { glyph: Icons.disc; size: Metrics.iconMd; color: Theme.color.accent }
        Text { text: row.headerLabel; font.pointSize: Type.meta; font.weight: Type.semibold; color: Theme.color.textPrimary }
        Text { text: row.headerMeta; font.pointSize: Type.meta; color: Theme.color.textTertiary }
    }

    Item {
        anchors.fill: parent
        visible: !row.header

        Text {
            x: row.cols.indexX
            width: row.cols.index - Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            visible: !row.playing
            text: row.number > 0 ? row.number : ""
            horizontalAlignment: Text.AlignRight
            font.pointSize: Type.small
            font.features: { "tnum": 1 }
            color: Theme.color.textTertiary
        }

        PlayingIndicator {
            x: row.cols.indexX + row.cols.index - Metrics.gap8 - width
            anchors.verticalCenter: parent.verticalCenter
            visible: row.playing
        }

        // Only a rich row with covers builds one: a compact row carries no
        // image and no shader at all.
        Loader {
            active: row.cols.art > 0
            x: row.cols.artX
            anchors.verticalCenter: parent.verticalCenter
            sourceComponent: ArtTile {
                width: Metrics.artRow
                height: Metrics.artRow
                radius: Metrics.radiusArtSmall
                source: row.art
                tint: row.index
            }
        }

        Column {
            x: row.cols.titleX
            width: row.cols.titleW - Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter

            Text {
                width: parent.width
                text: row.title
                elide: Text.ElideRight
                font.pointSize: row.rich ? Type.bodyLarge : Type.row
                font.weight: row.rich ? Type.medium : Type.regular
                color: row.playing ? Theme.color.accent : Theme.color.textPrimary
            }
            Text {
                width: parent.width
                visible: row.rich
                text: row.artist
                elide: Text.ElideRight
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }

        Text {
            x: row.cols.artistX
            width: row.cols.artistW - Metrics.gap12
            visible: width > 0
            anchors.verticalCenter: parent.verticalCenter
            text: row.artist
            elide: Text.ElideRight
            font.pointSize: Type.row
            color: row.playing ? Theme.color.accent : Theme.color.textPrimary
        }

        Text {
            x: row.cols.albumX
            width: row.cols.albumW - Metrics.gap12
            visible: width > 0
            anchors.verticalCenter: parent.verticalCenter
            text: row.album
            elide: Text.ElideRight
            font.pointSize: Type.row
            color: row.playing ? Theme.color.accent : row.rich ? Theme.color.textSecondary : Theme.color.textPrimary
        }

        Row {
            x: row.cols.qualityX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap6

            QualityIcon { tier: row.tier }
            Text { text: row.quality; font.pointSize: Type.small; color: Theme.color.textSecondary }
        }

        Text {
            x: row.cols.playsX
            width: row.cols.plays - Metrics.gap12
            visible: row.cols.plays > 0
            anchors.verticalCenter: parent.verticalCenter
            text: row.plays
            horizontalAlignment: Text.AlignRight
            font.pointSize: Type.small
            font.features: { "tnum": 1 }
            color: Theme.color.textTertiary
        }

        Text {
            x: row.cols.timeX
            width: row.cols.time
            anchors.verticalCenter: parent.verticalCenter
            text: row.duration
            horizontalAlignment: Text.AlignRight
            font.pointSize: Type.row
            font.features: { "tnum": 1 }
            color: row.playing ? Theme.color.accent : row.rich ? Theme.color.textSecondary : Theme.color.textPrimary
        }
    }
}
