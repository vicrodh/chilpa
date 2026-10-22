pragma Singleton
import QtQuick
import app.chilpa
import app.chilpa.kit

QtObject {
    property var rows: [{ kind: "Playlist", id: 7, name: "Stored playlist", detail: "", source: "", source_name: "local", artwork_path: "", covers: [], color: "", track_count: 3, play_count: 0, folder_id: null, local: true, visible_sidebar: true }]
    // The source each answer was asked for, as the facade filters by it.
    property string askedSource: ""
    function json(query, filter, source, order, ascending, folderId) {
        askedSource = source
        return JSON.stringify(rows.filter(row => source === "" || (source === "local" ? row.local : row.source === source)))
    }
    function sourcesJson() { return JSON.stringify([...new Set(rows.map(row => row.local ? "local" : row.source))]) }
    // The order each playlist keeps, as the facade does.
    property var trackOrders: ({})
    function trackOrder(id) { return trackOrders[id] || "custom" }
    function setTrackOrder(id, order) { const next = Object.assign({}, trackOrders); next[id] = order; trackOrders = next }
    function groupSources(grouped) { Playlists.catalog = Object.assign({}, Playlists.catalog, {group_sources:grouped}); SettingsModel.keep("navigation.groupSourcePlaylists", String(grouped)); Prefs.changed() }
}
