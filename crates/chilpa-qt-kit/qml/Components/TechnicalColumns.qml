import QtQuick
import app.chilpa.kit
import "../Theme"

// I share column boundaries between the album table's header and its rows.
QtObject {
    property real tableWidth: Metrics.nerdyTableWidth
    readonly property var fields: [
        { key: "tracks", width: Metrics.colPlaysCompact, label: I18n.tr("Tracks", I18n.rev) },
        { key: "format", width: Metrics.nerdyFormat, label: I18n.tr("Container", I18n.rev) },
        { key: "codec", width: Metrics.nerdyCodec, label: I18n.tr("Codec", I18n.rev) },
        { key: "rate", width: Metrics.nerdyRate, label: I18n.tr("Sample rate", I18n.rev) },
        { key: "bits", width: Metrics.nerdyDepth, label: I18n.tr("Bits", I18n.rev) },
        { key: "channels", width: Metrics.nerdyChannels, label: I18n.tr("Channels", I18n.rev) },
        { key: "bytes", width: Metrics.nerdySize, label: I18n.tr("File size", I18n.rev) },
        { key: "duration_ms", width: Metrics.colPlaysRich, label: I18n.tr("Duration", I18n.rev) },
        { key: "year", width: Metrics.nerdyYear, label: I18n.tr("Year", I18n.rev) },
        { key: "source", width: Metrics.nerdySource, label: I18n.tr("Source", I18n.rev) },
        { key: "artwork", width: Metrics.nerdyArtwork, label: I18n.tr("Artwork", I18n.rev) }
    ]
    readonly property real textWidth: tableWidth - fields.reduce((sum, field) => sum + field.width, 0)
    readonly property var columns: layout([
        { key: "album", width: textWidth * 0.6, label: I18n.tr("Album", I18n.rev) },
        { key: "artist", width: textWidth * 0.4, label: I18n.tr("Artist", I18n.rev) }
    ].concat(fields))
    function layout(fields) {
        let at = 0
        return fields.map(field => {
            const placed = Object.assign({x: at}, field)
            at += field.width
            return placed
        })
    }
    function display(key, value) {
        if (value === undefined || value === null || value === "" || value === 0) return "—"
        if (key === "artwork") return value ? I18n.tr("Yes", I18n.rev) : I18n.tr("No", I18n.rev)
        if (key === "source" && value === "local") return I18n.tr("Local", I18n.rev)
        if (key === "rate") return Number(value).toLocaleString(Qt.locale(), "f", 0) + " Hz"
        if (key === "bytes") return (value / (1024 * 1024)).toFixed(1) + " MiB"
        if (key === "duration_ms") {
            const seconds = Math.round(value / 1000), two = n => (n < 10 ? "0" : "") + n
            return two(Math.floor(seconds / 3600)) + ":" + two(Math.floor(seconds / 60) % 60) + ":" + two(seconds % 60)
        }
        return String(value)
    }
}
