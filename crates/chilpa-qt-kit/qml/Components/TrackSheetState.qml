import QtQuick
import app.chilpa.kit

// Each Tracks page owns its filters; column visibility survives restarts.
QtObject {
    id: state
    required property var settings
    required property var visits
    property var filters: visits.recall("tracks.columnFilters", ({}))
    property var columns: readColumns(settings.kept("ui.tracks.columns"))
    readonly property var defaults: ["index", "art", "source", "time", "rating", "quality", "favorite"]
    readonly property bool filtered: Object.keys(effective).some(key => {
        const value = effective[key]
        return typeof value === "boolean" || Array.isArray(value) && value.length > 0
    })
    readonly property var effective: {
        const next = Object.assign({}, filters)
        if (!Prefs.ratings) delete next.rating
        return next
    }
    function readColumns(text) {
        try { const parsed = JSON.parse(text); if (Array.isArray(parsed)) return parsed } catch (error) {}
        return defaults.slice()
    }
    function apply(field, values) {
        const next = Object.assign({}, filters)
        if (values === null || Array.isArray(values) && values.length === 0) delete next[field]
        else next[field] = values
        filters = next
    }
    function keepColumns(chosen) { columns = chosen; settings.keep("ui.tracks.columns", JSON.stringify(chosen)) }
    Component.onDestruction: visits.keep("tracks.columnFilters", filters)
}
