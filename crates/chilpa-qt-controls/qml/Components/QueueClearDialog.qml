import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// Capture the queue before asking, including the live queue of the side panel.
ConfirmationDialog {
    id: dialog
    property string queueId: ""
    property bool liveQueue: false
    property bool removeQueue: false
    readonly property var document: JSON.parse(ListenList.snapshot || "{}")
    function show(target, live, remove) {
        ListenList.refresh()
        liveQueue = live === true
        removeQueue = remove === true
        queueId = String(target || (liveQueue ? document.active : document.selected) || "")
        const queue = (document.tabs || []).find(tab => tab.id === queueId)
        title = removeQueue ? I18n.trArgs("Close “{}”?", [queue ? queue.name : ""], I18n.rev)
            : I18n.trArgs("Clear “{}”?", [queue ? queue.name : ""], I18n.rev)
        open()
    }
    targetValid: queueId !== "" && (document.tabs || []).some(tab => tab.id === queueId)
        && (!liveQueue || document.active === queueId)
    message: removeQueue ? I18n.tr("This queue and its saved order will be removed.", I18n.rev)
        : I18n.tr("All tracks will be removed from this queue. Playback from this queue will stop.", I18n.rev)
    confirmText: removeQueue ? I18n.tr("Close queue", I18n.rev) : I18n.tr("Clear queue", I18n.rev)
    destructive: true
    onConfirmed: {
        if (liveQueue) Player.clearQueue()
        else ListenList.command(removeQueue ? "close" : "clear", queueId, "")
    }
}
