pragma Singleton
import QtQuick
import app.chilpa
import app.chilpa.kit

QtObject {
    property string answer: JSON.stringify([{ kind: "Playlist", id: 7, name: "Stored playlist", detail: "", source: "", source_name: "Local", artwork_path: "", covers: [], color: "", track_count: 3, play_count: 0, folder_id: null, local: true, visible_sidebar: true }])
    function json(query, filter, order, ascending, folderId) { return answer }
    function groupSources(grouped) { Playlists.catalog = Object.assign({}, Playlists.catalog, {group_sources:grouped}); SettingsModel.keep("navigation.groupSourcePlaylists", String(grouped)); Prefs.changed() }
}
