import QtQuick
import app.chilpa.playlists
// Where a menu asks for the add-to-playlist picker. The picker itself is
// mounted once in the window and outlives the menus, as QBZ's is; a menu
// gives it the exact library track ids it carries.
Item {
    property string trackIdsJson: "[]"
    function show(left, top) { AddToPlaylist.show(trackIdsJson) }
    function beginCreate() { AddToPlaylist.show("[]") }
    width: 0
    height: 0
    visible: false
}
