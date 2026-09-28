pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property var tracks: ({})
    property var albums: ({})
    property bool fail: false
    function track(id, unused) { return tracks[id] || 0 }
    function album(id, unused) { return albums[id] || 0 }
    function rateTrack(id, stars) { if (!fail) { tracks[id] = stars; ++rev } }
    function rateAlbum(id, stars) { if (!fail) { albums[id] = stars; ++rev } }
}
