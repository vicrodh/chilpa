// hot-path: one per record in a list of records, recycled while it scrolls.
import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A record as a row: an arrow that opens its quick view under it, its cover,
// its title and who made it, then the columns asked for, in this order:
// year, tracks, length, genre, stars and the sources it is on. A click
// anywhere but the arrow and the stars opens the record.
Loader {
    id: row

    property int albumId
    property bool explicitContent: false
    property bool aiGenerated: false
    property string title
    property string subtitle
    property string year
    property url art
    property int tint: 0
    property bool technical: false
    property var technicalColumns: null
    readonly property var technicalFacts: (technical || shows("rate") || shows("bits")) && albumId !== 0 ? JSON.parse(AlbumActions.technical(albumId, rev) || "{}") : ({})
    // The columns shown, by name: "year", "tracks", "duration", "genre",
    // "rating", "source".
    property var columns: ["year"]
    // The grid's revision: what the record holds is read again when it moves.
    property int rev: 0
    // Tracks, length and genre, read only when a column shows one.
    readonly property var facts: !technical && albumId !== 0 && (shows("tracks") || shows("duration") || shows("genre"))
        ? JSON.parse(AlbumActions.facts(albumId, rev) || "{}") : ({})
    property int tracks: facts.tracks || 0
    property real durationMs: facts.duration_ms || 0
    property string genre: facts.genre || ""
    property int stars: !technical && albumId !== 0 && shows("rating") ? Ratings.stars("album", albumId, Ratings.rev) : 0
    // The record's sources; their column only where a list mixes them.
    property var sources: []
    property bool mixed: false

    property bool selected: false
    property bool expanded: false
    // Nothing of it plays now (ADR-017): dimmed, and a click says why.
    property bool available: true

    // A click outside the arrow, with the modifiers held: 1 for Ctrl or
    // Cmd, 2 for Shift.
    signal clicked(int modifiers)
    signal quick()
    signal menu(real x, real y)

    function shows(column) { return row.columns.indexOf(column) >= 0 }

    opacity: available ? 1 : Metrics.disabledOpacity

    ListRowSurface {
        z: -1
        anchors.fill: parent
        radius: Metrics.radiusRow
        rowIndex: row.tint
        hovered: hover.hovered
        selected: row.selected
        emphasized: row.expanded
    }

    sourceComponent: technical ? technicalBody : richBody
    Component {
        id: richBody
        Item {
            implicitHeight: Metrics.rowRich
            readonly property alias artistItem: artistLabel
            readonly property alias ratingItem: cells.ratingItem
            Item {
                id: arrow
                width: Metrics.button32
                height: parent.height

                Icon {
                    anchors.centerIn: parent
                    glyph: Icons.chevronRight
                    size: Metrics.iconSm
                    color: over.hovered ? Theme.color.textPrimary : Theme.color.textSecondary
                    rotation: row.expanded ? 90 : 0
                }
                HoverHandler { id: over; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    enabled: row.available
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onTapped: row.quick()
                }
            }
            ArtTile {
                id: cover
                anchors.left: arrow.right
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.artRow
                height: Metrics.artRow
                radius: Metrics.radiusArtSmall
                source: row.art
                tint: row.tint
            }
            Column {
                anchors.left: cover.right
                anchors.leftMargin: Metrics.gap12
                anchors.right: cells.left
                anchors.rightMargin: Metrics.gap12
                anchors.verticalCenter: parent.verticalCenter

                AdvisoryTitle {
                    explicitContent: row.explicitContent
                    aiGenerated: row.aiGenerated
                    width: parent.width
                    text: row.title
                    elide: Text.ElideRight
                    font.pointSize: Type.bodyLarge
                    font.weight: Type.medium
                    color: Theme.color.textPrimary
                }
                AlbumArtistText {
                    id: artistLabel
                    width: parent.width
                    text: row.subtitle
                    albumId: row.technical ? 0 : row.albumId; onFallback: row.clicked(0)
                    elide: Text.ElideRight
                    font.pointSize: Type.meta
                }
            }
            AlbumListCells {
                id: cells
                anchors.right: parent.right
                anchors.rightMargin: Metrics.gap12
                height: parent.height
                columns: row.columns
                facts: row.technicalFacts
                year: row.year
                tracks: row.tracks
                durationMs: row.durationMs
                genre: row.genre
                stars: row.stars
                available: row.available
                mixed: row.mixed
                sources: row.sources
                onRated: (value) => Ratings.rate("album", row.albumId, value)
            }
        }
    }
    Component {
        id: technicalBody
        TechnicalCells { implicitHeight: Metrics.rowCompact; shown: row.technicalColumns; explicitContent: row.explicitContent; aiGenerated: row.aiGenerated; facts: row.technicalFacts; title: row.title; artist: row.subtitle; year: row.year }
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler {
        id: tap
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => {
            const inside = (item) => {
                const at = item.mapFromItem(row, point.position.x, point.position.y)
                return item.visible && at.x >= 0 && at.x <= item.width && at.y >= 0 && at.y <= item.height
            }
            if (button === Qt.LeftButton && !row.technical && row.item && (inside(row.item.artistItem) || inside(row.item.ratingItem))) return
            if (button === Qt.RightButton)
                return row.menu(point.position.x, point.position.y)
            // The arrow is its own place.
            if (!row.technical && point.position.x < Metrics.button32)
                return
            const held = tap.point.modifiers
            row.clicked((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0))
        }
    }
}
