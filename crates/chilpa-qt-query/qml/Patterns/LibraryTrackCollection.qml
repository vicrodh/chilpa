import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// Enter only after the requested collection arrives. Subsequent refreshes
// keep the same viewport and delegates until the mode or screen is left.
Loader {
    id: collection
    property TrackListModel model
    property bool published: false
    Component.onCompleted: published = Library.albumless && !Library.busy
    property int entry: 0
    active: Library.albumless && published && visible

    Connections {
        target: Library
        function onAlbumlessChanged() {
            collection.entry++
            collection.published = false
        }
        function onBusyChanged() {
            if (Library.busy || !Library.albumless || collection.published) return
            const entry = collection.entry
            // arrive publishes the snapshot before busy=false and its rev
            // immediately afterwards. Wait until the whole publication ends.
            Qt.callLater(() => {
                if (collection.entry === entry && Library.albumless && !Library.busy)
                    collection.published = true
            })
        }
    }
    sourceComponent: Component {
        TrackList {
            objectName: "albumlessTracks"
            model: collection.model
            showAlbum: false
            bottomInset: Metrics.contentBottom
            Text {
                anchors.centerIn: parent
                visible: !Library.busy && collection.model.count === 0
                text: I18n.tr("No tracks without an album match these filters.", I18n.rev)
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
        }
    }
}
