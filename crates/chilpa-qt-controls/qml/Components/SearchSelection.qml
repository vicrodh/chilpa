import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// I keep loading and failure visible until the request finishes or is cancelled.
Item {
    id: root
    required property var search
    Timer {
        interval: Motion.poll
        repeat: true
        running: root.search !== null && root.search.selectionPending
        onTriggered: root.search.pollSelection()
    }
    Connections {
        target: root.search
        function onPlaylistReady(tracks) { AddToPlaylist.show(tracks) }
        function onSelectionPendingChanged() { root.update() }
        function onSelectionErrorChanged() { root.update() }
    }
    function update() {
        if (search && (search.selectionPending || search.selectionError !== "")) waiting.open()
        else waiting.close()
    }
    Modal {
        id: waiting
        objectName: "searchSelectionWait"
        width: Metrics.confirmWidth
        height: body.implicitHeight + Metrics.modalInsets
        onClosed: if (root.search && (root.search.selectionPending || root.search.selectionError !== "")) root.search.cancelSelection()
        Column {
            id: body
            width: parent.width
            spacing: Metrics.gap12
            Text {
                width: parent.width
                wrapMode: Text.Wrap
                text: root.search && root.search.selectionError !== "" ? root.search.selectionError : I18n.tr("Reading selected tracks…", I18n.rev)
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            PillButton {
                text: I18n.tr("Cancel", I18n.rev)
                onClicked: { root.search.cancelSelection(); waiting.close() }
            }
        }
    }
}
