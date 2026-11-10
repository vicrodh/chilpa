import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// Optional metadata cells shared by compact album lists.
Row {
    id: cells
    property var columns: []
    property var facts: ({})
    property string year
    property int tracks: 0
    property real durationMs: 0
    property string genre
    property int stars: 0
    property bool available: true
    property bool mixed: false
    property var sources: []
    readonly property alias ratingItem: stars
    signal rated(int value)
    spacing: Metrics.gap16
    function technicalText(key) {
        if ((facts.mixed_fields || []).indexOf(key) >= 0) return I18n.tr("Mixed", I18n.rev)
        const value = facts[key]
        if (!value) return "—"
        return key === "rate" ? Number(value).toLocaleString(Qt.locale(), "f", 0) + " Hz" : String(value)
    }
    function shows(column) { return cells.columns.indexOf(column) >= 0 }
    // hh:mm:ss, as long records run past the hour.
    function clock(ms) {
        const s = Math.round(ms / 1000), two = (n) => (n < 10 ? "0" : "") + n
        return two(Math.floor(s / 3600)) + ":" + two(Math.floor(s / 60) % 60) + ":" + two(s % 60)
    }

    component Cell: Text {
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        font.pointSize: Type.row
        font.features: { "tnum": 1 }
        color: Theme.color.textSecondary
    }
    Cell { objectName: "yearCell"; visible: cells.shows("year"); width: Metrics.colTimeRich; text: cells.year }
    Cell { objectName: "tracksCell"; visible: cells.shows("tracks"); width: Metrics.colTimeRich; text: cells.tracks > 0 ? cells.tracks : "" }
    Cell { objectName: "durationCell"; visible: cells.shows("duration"); width: Metrics.colPlaysCompact; text: cells.durationMs > 0 ? cells.clock(cells.durationMs) : "" }
    Cell {
        objectName: "genreCell"; visible: cells.shows("genre"); width: Metrics.colQualityRich
        horizontalAlignment: Text.AlignLeft; text: cells.genre
    }
    Cell { objectName: "rateCell"; visible: cells.shows("rate"); width: Metrics.nerdyRate; text: visible ? cells.technicalText("rate") : "" }
    Cell { objectName: "bitsCell"; visible: cells.shows("bits"); width: Metrics.nerdyDepth; text: visible ? cells.technicalText("bits") : "" }
    RatingStars {
        id: stars
        objectName: "ratingCell"
        visible: cells.shows("rating")
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.colRating
        interactive: cells.available
        value: cells.stars
        onRated: (value) => cells.rated(value)
    }
    Row {
        objectName: "sourceCell"
        visible: cells.mixed && cells.shows("source")
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.colSource * 2 + Metrics.gap12 // derived: room for six sources side by side
        layoutDirection: Qt.RightToLeft
        spacing: Metrics.gap4
        Repeater {
            model: cells.sources
            SourceIcon {
                required property var modelData
                width: Metrics.iconSm + Metrics.gap4; height: width
                sourceInfo: modelData
            }
        }
    }
}
