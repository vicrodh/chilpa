import QtQuick
import com.blitzfc.chilpa
// A lightweight host adapter for the application-wide picker dialog. Menus
// give it the exact library track ids they carry; the dialog outlives them.
Item {
    property var trackIds: []
    function show(left, top) {
        dialog.show(trackIds)
    }
    function beginCreate() { dialog.beginCreate() }
    width: 0
    height: 0
    visible: false
    PlaylistPickerDialog { id: dialog }
}
