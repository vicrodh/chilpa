import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml"

ApplicationWindow {
    id: window
    visible: true; width: 640; height: 480
    readonly property string firstTitle: records.title(0, records.rev)
    readonly property int firstId: records.albumId(0, records.rev)
    property string publishedTitle: ""
    property int publishedId: 0
    ScreenHost { anchors.fill: parent }
    AlbumGridModel {
        id: records
        kind: "source"; key: "app.test.navigation section:albums"
        limit: 1; columns: 1; contentRev: Library.remoteRev + Library.remoteArtRev
    }
    function fail(message) { console.log("SMOKE FAIL " + message); Qt.exit(1) }
    function verifyCommittedCard() {
        const title = records.title(0, records.rev)
        const id = records.albumId(0, records.rev)
        if (firstTitle !== title || firstId !== id || publishedTitle !== title || publishedId !== id) {
            fail("revision published stale album bindings: " + firstTitle + "/" + firstId + " expected " + title + "/" + id)
            return
        }
        console.log("SMOKE album revision completed title=" + title + " id=" + id)
        Qt.exit(0)
    }
    Connections {
        target: records
        function onRevChanged() {
            window.publishedTitle = records.title(0, records.rev)
            window.publishedId = records.albumId(0, records.rev)
        }
        function onModelReset() {
            if (records.title(0, records.rev)) Qt.callLater(window.verifyCommittedCard)
        }
    }
    Timer { interval: 100; running: true; repeat: true; onTriggered: Sources.refresh() }
    Timer { interval: 15000; running: true; onTriggered: window.fail("album revision timed out") }
}
