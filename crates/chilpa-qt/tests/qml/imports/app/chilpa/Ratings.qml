pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property var tracks: ({})
    property var albums: ({})
    property var favorites: ({})
    property bool fail: false
    property var pendingTracks: ({})
    function pending(kind, key, unused) { return kind === "track" && !!pendingTracks[key] }
    function stars(kind, id, unused) { return (kind === "track" ? tracks[id] : albums[id]) || 0 }
    function rate(kind, id, stars) { if (!fail) { (kind === "track" ? tracks : albums)[id] = stars; ++rev } }
    function refresh() {}
    function markable(kind, key, unused) { return Number(key) > 0 || !!sources[key] }
    property var sources: ({})
    function favorite(kind, key, unused) { return !!favorites[kind + ":" + key] }
    function setFavorite(kind, key, on) { if (!fail) { favorites[kind + ":" + key] = on; ++rev } }
}
