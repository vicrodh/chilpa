import QtQuick
import app.chilpa
import app.chilpa.ambient

// I retain palette demand only while this surface can use its colors.
Item {
    id: follower
    required property string consumer
    property bool active: false
    property url art: Player.art
    function sync() {
        if (consumer !== "") Ambient.watch(consumer, art, active)
    }
    onActiveChanged: sync()
    onArtChanged: sync()
    Component.onCompleted: sync()
    Component.onDestruction: Ambient.watch(consumer, art, false)
}
