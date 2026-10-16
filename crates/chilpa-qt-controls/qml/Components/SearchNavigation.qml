import QtQuick
import app.chilpa

// I capture the lead in the signal and flush after its model invokable returns.
Connections {
    required property var search
    property var pending: []
    target: search

    function flush() {
        const ready = pending
        pending = []
        try {
            for (const lead of ready) {
                switch (lead.kind) {
                case "screen": Navigation.go(lead.target, lead.id); break
                case "tab": Navigation.open(lead.target, lead.id); break
                case "artist": Navigation.openArtist(lead.target); break
                case "credit": Navigation.openCredit(lead.target, lead.id); break
                case "folder": Navigation.openFolder(lead.target); break
                case "source": Navigation.goKey("source", lead.target); break
                }
            }
        } finally {
            for (const lead of ready)
                if (lead.model) lead.model.releaseNavigation(lead.token)
        }
    }
    function onNavigated(kind, target, id, token) {
        pending.push({model: this.search, kind: kind, target: target, id: id, token: token})
        Qt.callLater(flush)
    }
}
