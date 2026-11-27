pragma Singleton
import QtQuick

// The facade's removed albums as a test sets them, and what was asked of it.
QtObject {
    id: removed
    property int rev: 0
    property var albums: []
    property var removedIds: []
    property var restoredIds: []
    property int requests: 0
    property bool requested: false
    signal asked(real id, string title)
    signal listed()
    function ask(id, title) { if (id > 0) asked(id, title) }
    function remove(id) { removedIds = removedIds.concat([id]); rev++; return true }
    function restore(id) { restoredIds = restoredIds.concat([id]); albums = albums.filter(album => album.id !== id); rev++; return true }
    function json(rev) { return JSON.stringify(albums) }
    function takeRequest() { requests++; if (requested) { requested = false; listed() } }
}
