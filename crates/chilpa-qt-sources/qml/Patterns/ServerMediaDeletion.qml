import QtQuick
import app.chilpa.sources

// One window owns one optional confirmation. Closed state holds no dialog.
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
