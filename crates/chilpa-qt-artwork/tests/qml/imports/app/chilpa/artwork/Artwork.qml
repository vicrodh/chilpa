pragma Singleton
import QtQuick

// The artwork bridge as the tests see it: what a test sets is what it says,
// and each request is counted.
QtObject {
    property int rev: 0
    property var chosenAlbums: ({})
    property var addresses: ({})
    property var artists: ({})
    property var asked: []
    function albumChosen(albumId, rev) { return chosenAlbums[albumId] === true }
    function albumAddress(albumId, rev) { return addresses[albumId] || "" }
    function chooseAlbum(albumId) { asked = asked.concat(["choose " + albumId]) }
    function forgetAlbum(albumId) { asked = asked.concat(["forget " + albumId]) }
    function saveAlbum(albumId, title) { asked = asked.concat(["save " + albumId]) }
    function artist(name, rev) { return artists[name] || "" }
    function chooseArtist(name) { asked = asked.concat(["choose " + name]) }
    function forgetArtist(name) { asked = asked.concat(["forget " + name]) }
    function saveArtist(name, shown) { asked = asked.concat(["save " + name]) }
}
