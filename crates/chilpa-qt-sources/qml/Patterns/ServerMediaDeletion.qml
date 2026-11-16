import QtQuick
import app.chilpa
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/offline/qml/Components"

// One window owns one optional confirmation. Closed state holds no dialog.
// Music servers' "Offline music" leads to the Offline music page; a menu's
// offline actions stay where the listener is, and the window's one look at
// them keeps every row's mark current.
Item {
    id: host
    property bool listenForRequests: true
    property var requested: null
    Connections {
        target: Sources
        enabled: host.listenForRequests
        function onServerMediaRequested(kind, id) {
            if (host.requested === null) host.requested = {kind:kind,id:id}
        }
    }
    Connections {
        target: Offline
        enabled: host.listenForRequests
        function onRequested() { Navigation.go("offlineMusic", 0) }
    }
    OfflineActivity { listening: host.listenForRequests }
    Loader {
        active: host.requested !== null
        sourceComponent: ServerMediaDeleteDialog {
            kind: host.requested ? host.requested.kind : "track"
            localId: host.requested ? host.requested.id : 0
            Component.onCompleted: open()
            onClosed: host.requested = null
        }
    }
}
