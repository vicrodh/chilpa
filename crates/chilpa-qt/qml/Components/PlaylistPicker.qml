import QtQuick
import com.blitzfc.chilpa
// A lightweight host adapter for the application-wide picker dialog. Menus
// give it the exact library track ids they carry; the dialog outlives them.
//
// Every track list and every album menu has one of these, and most are
// never opened: the dialog, with its list and its search, is built the first
// time it is asked for and kept from then on.
Item {
    property var trackIds: []
    function show(left, top) {
        host.active = true
        host.item.show(trackIds)
    }
    function beginCreate() {
        host.active = true
        host.item.beginCreate()
    }
    width: 0
    height: 0
    visible: false
    Loader {
        id: host
        active: false
        sourceComponent: PlaylistPickerDialog {}
    }
}
