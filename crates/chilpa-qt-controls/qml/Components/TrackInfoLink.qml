import QtQuick
import app.chilpa

// The place a click opens what is known of the track that plays. It fills the
// item it sits in, usually the title, and loads the information only while
// it is open.
Item {
    anchors.fill: parent

    HoverHandler { cursorShape: Player.path !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler { objectName: "playerTitleInfo"; enabled: Player.path !== ""; onTapped: host.active = true }
    Loader {
        id: host
        objectName: "trackInfoHost"
        active: false
        sourceComponent: TrackInfoModal { onClosed: Qt.callLater(() => host.active = false) }
        onLoaded: item.open()
    }
}
