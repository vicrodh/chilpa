import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"

// Where an album taken out of the library is asked about first, and where
// the list of removed albums opens when Settings > Library asks for it.
// The window holds one.
Item {
    id: host

    Connections {
        target: Prefs
        function onRevChanged() { RemovedAlbums.takeRequest() }
    }
    Connections {
        target: RemovedAlbums
        function onAsked(id, title) {
            removal.albumId = id
            removal.albumTitle = title
            removal.open()
        }
        function onListed() { removed.open() }
    }
    ConfirmationDialog {
        id: removal
        objectName: "albumRemovalConfirmation"
        property real albumId: 0
        property string albumTitle
        title: I18n.trArgs("Remove “{}” from the library?", [albumTitle], I18n.rev)
        message: I18n.tr("Its files stay where they are, and it can come back from Settings > Library > Removed albums.", I18n.rev)
        confirmText: I18n.tr("Remove", I18n.rev)
        destructive: true
        onConfirmed: RemovedAlbums.remove(albumId)
    }
    RemovedAlbumsDialog { id: removed }
}
