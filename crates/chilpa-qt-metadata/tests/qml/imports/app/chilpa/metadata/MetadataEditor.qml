pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property bool open: false
    property bool discogsQuota: false
    property int quotaTaken: 0
    function takeDiscogsQuota() { if (!discogsQuota) return false; discogsQuota = false; quotaTaken++; return true }
    // What the facade's editor says, as the screen reads it.
    property var view: ({ state: "closed", album_id: 0, dirty: false, error: "" })
    property var opened: []
    property int saved: 0
    property int closed: 0
    function document(rev) { return JSON.stringify(view) }
    function openAlbum(album, track) { opened = opened.concat([album]) }
    function save() { ++saved }
    function close() { ++closed }
    function show(next) { view = next; ++rev }
}
