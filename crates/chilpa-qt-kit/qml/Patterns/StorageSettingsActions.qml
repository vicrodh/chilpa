import QtQuick
import app.chilpa.kit
import "../Theme"

// Cleanup actions share one confirmation; usage is polled only while
// the Storage section is on screen. All filesystem work stays in the facade.
Item {
    id: storage
    property var rows
    property bool current: false

    function press(key) {
        if (key.startsWith("storage.clear.")) {
            removal.kind = key.substring("storage.clear.".length)
            removal.open()
            return true
        }
        return false
    }

    ConfirmDialog {
        id: removal
        objectName: "storageClearConfirmation"
        property string kind: ""
        title: kind === "remote" ? I18n.tr("Clear temporary remote images?", I18n.rev)
            : kind === "lyrics" ? I18n.tr("Clear lyrics cache?", I18n.rev)
            : I18n.tr("Clear generated images?", I18n.rev)
        message: kind === "remote"
            ? I18n.tr("Images from sources and plugins will download again when needed. Artwork saved in your library stays.", I18n.rev)
            : kind === "lyrics" ? I18n.tr("Saved lyrics and translations will be fetched again when needed.", I18n.rev)
            : I18n.tr("Chilpa will make smaller covers and backgrounds again when needed.", I18n.rev)
        confirmText: I18n.tr("Clear", I18n.rev)
        destructive: true
        onConfirmed: storage.rows.press("storage.clear." + kind)
    }
    Timer {
        interval: 1000
        repeat: true
        running: storage.current
        onTriggered: storage.rows.refresh()
    }
}
