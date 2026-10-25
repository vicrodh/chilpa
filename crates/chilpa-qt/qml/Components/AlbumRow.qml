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
Item {
    id: row

    property int albumId
    property string title
    property string subtitle
    property string year
    property url art
    property int tint: 0
    property bool technical: false
    readonly property var technicalFacts: technical && albumId !== 0 ? JSON.parse(AlbumActions.technical(albumId, rev) || "{}") : ({})
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
    // hh:mm:ss, as long records run past the hour.
    function clock(ms) {
        const s = Math.round(ms / 1000), two = (n) => (n < 10 ? "0" : "") + n
        return two(Math.floor(s / 3600)) + ":" + two(Math.floor(s / 60) % 60) + ":" + two(s % 60)
    }

    implicitHeight: technical ? Metrics.rowCompact : Metrics.rowRich
    opacity: available ? 1 : Metrics.disabledOpacity

    ListRowSurface {
        anchors.fill: parent
        radius: Metrics.radiusRow
        rowIndex: row.tint
        hovered: hover.hovered
        selected: row.selected
        emphasized: row.expanded
    }

    Item {
        id: arrow
        visible: !row.technical
        width: row.technical ? 0 : Metrics.button32
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
    Loader {
        id: cover
        active: !row.technical
        anchors.left: arrow.right
        anchors.verticalCenter: parent.verticalCenter
        width: row.technical ? 0 : Metrics.artRow
        height: Metrics.artRow
        sourceComponent: ArtTile { radius: Metrics.radiusArtSmall; source: row.art; tint: row.tint }
    }
    Column {
        visible: !row.technical
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: cells.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter

        Text {
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
    component Cell: Text {
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        font.pointSize: Type.row
        font.features: { "tnum": 1 }
        color: Theme.color.textSecondary
    }
    Row {
        id: cells
        visible: !row.technical
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        height: parent.height
        spacing: Metrics.gap16

        Cell { objectName: "yearCell"; visible: row.shows("year"); width: Metrics.colTimeRich; text: row.year }
        Cell { objectName: "tracksCell"; visible: row.shows("tracks"); width: Metrics.colTimeRich; text: row.tracks > 0 ? row.tracks : "" }
        Cell { objectName: "durationCell"; visible: row.shows("duration"); width: Metrics.colPlaysCompact; text: row.durationMs > 0 ? row.clock(row.durationMs) : "" }
        Cell {
            objectName: "genreCell"; visible: row.shows("genre"); width: Metrics.colQualityRich
            horizontalAlignment: Text.AlignLeft; text: row.genre
        }
        RatingStars {
            id: stars
            objectName: "ratingCell"
            visible: row.shows("rating")
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.colRating
            interactive: row.available
            value: row.stars
            onRated: (value) => Ratings.rate("album", row.albumId, value)
        }
        Row {
            objectName: "sourceCell"
            visible: row.mixed && row.shows("source")
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.colSource * 2 + Metrics.gap12 // derived: room for six sources side by side
            layoutDirection: Qt.RightToLeft
            spacing: Metrics.gap4
            Repeater {
                model: row.sources
                SourceIcon {
                    required property var modelData
                    width: Metrics.iconSm + Metrics.gap4; height: width
                    sourceInfo: modelData
                }
            }
        }
    }

    Loader {
        anchors.fill: parent
        active: row.technical
        sourceComponent: TechnicalCells { facts: row.technicalFacts; title: row.title; artist: row.subtitle; year: row.year }
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
            if (button === Qt.LeftButton && (inside(artistLabel) || inside(stars))) return
            if (button === Qt.RightButton)
                return row.menu(point.position.x, point.position.y)
            // The arrow is its own place.
            if (point.position.x < cover.x)
                return
            const held = tap.point.modifiers
            row.clicked((held & (Qt.ControlModifier | Qt.MetaModifier) ? 1 : 0) | (held & Qt.ShiftModifier ? 2 : 0))
        }
    }
}
