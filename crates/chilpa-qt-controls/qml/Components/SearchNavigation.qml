import QtQuick
import app.chilpa

// I defer navigation until the model's invokable and signal have returned.
Connections {
    required property var search
    target: search
    function onNavigated(kind, target, id, token) {
        const model = this.search
        Qt.callLater(function() {
            try {
                switch (kind) {
                case "screen": Navigation.go(target, id); break
                case "tab": Navigation.open(target, id); break
                case "artist": Navigation.openArtist(target); break
                case "credit": Navigation.openCredit(target, id); break
                case "folder": Navigation.openFolder(target); break
                case "source": Navigation.goKey("source", target); break
                }
            } finally {
                if (model) model.releaseNavigation(token)
            }
        })
    }
}
