import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TrackRowSlot {
    id: track

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
    property string headerGlyph: ""
    property string headerIconFamily: ""
    property bool headerWork: false
    property string headerComposer: ""
    property string headerComposerTarget: ""
    required property url art
    // It cannot play now (ADR-017): dimmed, and the list does not play it.
    required property bool available
    required property double trackId

    property TrackColumns cols
    property bool hovered: false
    // Its artist is under the pointer: a click on it opens the artist.
    property bool artistHovered: false
    property bool albumHovered: false; property string sourceName: ""
    // Who wrote it, quiet after the title; empty when nobody is named.
    property string composer: ""
    property bool explicitContent: false
    property var sourceInfo: ({})
    property bool selectionMode: false
    // The first row of a group in a list whose group names stay at the top.
    property bool groupStart: false
    property bool groupActionsEnabled: false
    property int rating: 0
    property int ratingPreview: 0
    property bool favorite: false
    // The action under the pointer: 1 the heart, 2 the menu, 0 neither.
    property int actionHovered: 0
    // The arrow under the pointer: 1 up, 2 down, 0 neither.
    property int moveHovered: 0

    // A header: a divider (1), or one that stands for a record (2).
    header: rowKind !== 0
    readonly property bool record: rowKind === 2
    readonly property bool rich: cols.rich

    width: cols.width
    height: record ? Metrics.rowRecord : headerWork ? Metrics.rowWork : header ? Metrics.rowGroup : rich ? Metrics.rowRich : Metrics.rowCompact
    opacity: available ? 1 : Metrics.disabledOpacity

    content: Item {
        id: content
        objectName: "trackRowBody"
        readonly property var row: track
        readonly property alias creditedTitle: creditedTitle
        readonly property Text artistLabel: row.rich ? richArtist : compactArtist
        readonly property alias albumText: albumLabel
        readonly property alias sourceBadge: badge
        readonly property alias end: rowEnd
        readonly property alias ratingStars: rowFacts.ratingStars
        readonly property Item headerComposerLabel: groupHeader.composerLabel
        readonly property Item groupActionArea: groupHeader.actionArea
        function groupActionAt(from, x, y) { return groupHeader.actionAt(from, x, y) }

        Rectangle { objectName: "groupStart"; visible: row.groupStart; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline; z: 1 } // derived: over the row's surface
        ListRowSurface {
            anchors.fill: parent
            rowIndex: row.index
            radius: row.rich ? Metrics.radiusCard : Metrics.radiusRow
            visible: !row.header
            hovered: row.hovered; selected: row.selected
            emphasized: row.playing && row.rich
            edge: row.playing && !Prefs.animatedIndicator
        }

        TrackRowHeader { id: groupHeader; row: content.row; actionsEnabled: content.row.groupActionsEnabled }
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
                inViewport: row.immediate
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

                CreditedTitle {
                    id: creditedTitle
                    width: parent.width
                    title: row.title
                    explicitContent: row.explicitContent
                    credit: row.credits.featured_html || ""
                    composer: !Prefs.trackComposers ? "" : row.credits.links ? row.credits.composer_html || "" : row.composer || ""
                    richCredits: !!row.credits.links
                    titleSize: row.rich ? Type.bodyLarge : Type.row
                    titleWeight: row.rich ? Type.medium : Type.regular
                }
                Text {
                    id: richArtist
                    objectName: "trackPerformingArtistsRich"
                    width: parent.width
                    visible: row.rich
                    text: row.credits.artist_html || row.artist
                    textFormat: row.credits.links ? Text.RichText : Text.PlainText
                    linkColor: color
                    elide: Text.ElideRight
                    font.pointSize: Type.meta
                    color: row.artistHovered ? Theme.color.textPrimary : Theme.color.textSecondary
                }
            }

            Text {
                id: compactArtist
                objectName: "trackPerformingArtists"
                x: row.cols.artistX
                width: row.cols.artistW - Metrics.gap12
                visible: width > 0
                anchors.verticalCenter: parent.verticalCenter
                text: row.credits.artist_html || row.artist
                textFormat: row.credits.links ? Text.RichText : Text.PlainText
                linkColor: color
                elide: Text.ElideRight
                font.pointSize: Type.row
                color: Theme.color.textPrimary
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
                color: row.albumHovered || !row.rich ? Theme.color.textPrimary : Theme.color.textSecondary
            }

            TrackSourceLabel { id: badge; cols: row.cols; name: row.sourceName; sourceInfo: row.sourceInfo; anchors.verticalCenter: parent.verticalCenter }

            TrackRowFacts { id: rowFacts; row: content.row }

            TrackRowEnd {
                id: rowEnd
                x: row.cols.moreX
                height: parent.height
                visible: row.available
                trackId: row.trackId
                favorite: row.favorite
                inViewport: row.immediate
                hovered: row.actionHovered
            }
        }
    }
}
