import QtQuick
import app.chilpa.sources

// One window owns one optional confirmation. Closed state holds no dialog.
Item {
    id: host
    property bool listenForRequests: true
    property var requested: null
    property var offlineRequested: null
    Connections {
        target: Sources
        enabled: host.listenForRequests
        function onServerMediaRequested(kind, id) {
            if (host.requested === null) host.requested = {kind:kind,id:id}
        }
        function onOfflineRequested(kind, selected) {
            if (host.offlineRequested === null) host.offlineRequested = {kind:kind,selected:selected}
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
    Loader {
        active: host.offlineRequested !== null
        sourceComponent: ServerOfflineDialog {
            kind: host.offlineRequested ? host.offlineRequested.kind : "tracks"
            selected: host.offlineRequested ? host.offlineRequested.selected : "[]"
            Component.onCompleted: open()
            onClosed: host.offlineRequested = null
        }
    }
}
