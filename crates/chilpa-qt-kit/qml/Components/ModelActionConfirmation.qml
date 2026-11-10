import QtQuick
import app.chilpa
import app.chilpa.kit
import "../Theme"

// Removing playlist entries asks once and keeps the selected identities.
ConfirmationDialog {
    id: dialog
    property var target: null
    property int row: -1
    property string selection: ""
    function stamp() {
        if (!target) return ""
        return JSON.stringify([target.kind,target.albumId,target.key,target.order,target.ascending,
            target.count,target.filterRev,target.selectedCount,target.chosenTrackIdsJson(row)])
    }
    function show(model, at) {
        target = model
        row = at
        selection = stamp()
        open()
    }
    function forget() {
        target = null
        selection = ""
        row = -1
    }
    targetValid: target !== null && selection === stamp()
    checkTarget: () => target !== null && selection === stamp()
    destructive: true
    title: I18n.tr("Remove from playlist", I18n.rev)
    message: I18n.tr("The selected tracks will be removed from this playlist and its saved order.", I18n.rev)
    confirmText: I18n.tr("Remove", I18n.rev)
    onConfirmed: {
        target.removeChosen(row)
        forget()
    }
    onDismissed: forget()
}
