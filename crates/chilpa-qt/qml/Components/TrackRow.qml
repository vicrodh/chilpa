import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Components"

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
    // It cannot play now (ADR-017): dimmed, and the list does not play it.
    required property bool available
    required property double trackId

    property TrackColumns cols
    property bool hovered: false
    // Its artist is under the pointer: a click on it opens the artist.
    property bool artistHovered: false
    property bool albumHovered: false; property string sourceName: ""
    property var sourceInfo: ({}); readonly property Item sourceBadge: badge
    property bool selectionMode: false
    property int rating: 0
    property int ratingPreview: 0
    property bool canRate: false
    property bool favorite: false
    // The action under the pointer: 1 the heart, 2 the menu, 0 neither.
    property int actionHovered: 0
    // The arrow under the pointer: 1 up, 2 down, 0 neither.
    property int moveHovered: 0
    // Its heart and its menu button, which the list resolves the clicks of.
    readonly property TrackRowEnd end: rowEnd

    // A header: a divider (1), or one that stands for a record (2).
    readonly property bool header: rowKind !== 0
    readonly property bool record: rowKind === 2
    readonly property bool rich: cols.rich

    // The label that shows its artist, for the list to tell which one of
    // several a click is on.
    readonly property Text artistLabel: rich ? richArtist : compactArtist

    function containsArtist(x, y) {
        const label = rich ? richArtist : compactArtist
        const point = label.mapFromItem(row, x, y)
        return !header && label.visible && point.x >= 0 && point.y >= 0
            && point.x < Math.min(label.width, label.contentWidth)
            && point.y < Math.min(label.height, label.contentHeight)
    }

    function containsAlbum(x, y) {
        const point = albumLabel.mapFromItem(row, x, y)
        return !header && albumLabel.visible && point.x >= 0 && point.y >= 0
            && point.x < Math.min(albumLabel.width, albumLabel.contentWidth)
            && point.y < albumLabel.height
    }

    function ratingAt(x, y) {
        const point = stars.mapFromItem(row, x, y)
        return !header && canRate && stars.visible && point.x >= 0 && point.x < stars.width
            && point.y >= 0 && point.y < stars.height
            ? Math.floor(point.x / stars.width * Metrics.ratingMaximum) + 1 : 0
    }

    width: cols.width
    height: record ? Metrics.rowRecord : header ? Metrics.rowGroup : rich ? Metrics.rowRich : Metrics.rowCompact
    opacity: available ? 1 : Metrics.disabledOpacity

    ListRowSurface {
        anchors.fill: parent
        rowIndex: row.index
        radius: row.rich ? Metrics.radiusCard : Metrics.radiusRow
        visible: !row.header
        hovered: row.hovered
        selected: row.selected
        emphasized: row.playing && row.rich
    }

    // Only a header builds one; it stops short of the group's own actions.
    Loader {
        active: row.header
        x: row.cols.indexX
        width: row.width - x - row.cols.more - Metrics.gap16 // derived: what the actions take
        height: parent.height
        sourceComponent: TrackGroupHeader { objectName: "groupHeader"; of: row }
    }

    Item {
        anchors.fill: parent
        visible: !row.header

        TrackRowLead {
            x: row.cols.moveX
            width: row.cols.move + row.cols.index
            height: parent.height
            gutter: row.cols.move
            stacked: row.rich
            number: row.number
            playing: row.playing
            selected: row.selected
            selectionMode: row.selectionMode
            reorder: row.cols.reorder && row.available
            moveHovered: row.moveHovered
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
                id: richArtist
                width: parent.width
                visible: row.rich
                text: row.artist
                elide: Text.ElideRight
                font.pointSize: Type.meta
                color: row.artistHovered ? Theme.color.textPrimary : Theme.color.textSecondary
            }
        }

        Text {
            id: compactArtist
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
            id: albumLabel
            x: row.cols.albumX
            width: row.cols.albumW - Metrics.gap12
            visible: width > 0
            anchors.verticalCenter: parent.verticalCenter
            text: row.album
            elide: Text.ElideRight
            font.pointSize: Type.row
            color: row.playing ? Theme.color.accent : row.albumHovered ? Theme.color.textPrimary : row.rich ? Theme.color.textSecondary : Theme.color.textPrimary
        }

        TrackSourceLabel { id: badge; cols: row.cols; name: row.sourceName; sourceInfo: row.sourceInfo; anchors.verticalCenter: parent.verticalCenter }

        Row {
            x: row.cols.qualityX
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap6

            QualityIcon { tier: row.tier }
            Text { text: row.quality; font.pointSize: Type.small; color: Theme.color.textSecondary }
        }

        RatingStars {
            id: stars
            x: row.cols.ratingX
            anchors.verticalCenter: parent.verticalCenter
            visible: row.cols.rating > 0
            interactive: false
            value: row.rating
            pointerPreview: row.ratingPreview
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

        TrackRowEnd {
            id: rowEnd
            x: row.cols.moreX
            height: parent.height
            visible: row.available
            trackId: row.trackId
            favorite: row.favorite
            hovered: row.actionHovered
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
