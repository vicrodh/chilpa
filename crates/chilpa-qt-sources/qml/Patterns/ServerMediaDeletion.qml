import QtQuick
import app.chilpa
import app.chilpa.sources
import app.chilpa.offline

// One window owns one optional confirmation. Closed state holds no dialog.
// A menu's "Make available offline…" and Music servers' "Offline music"
// lead to the Offline music page, with what they chose staged for it.
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
        function onRequested(kind, selected) {
            Offline.call("stage", kind, selected)
            Navigation.go("offlineMusic", 0)
        }
    }
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
