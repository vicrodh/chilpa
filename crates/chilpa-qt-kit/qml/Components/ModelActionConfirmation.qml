import QtQuick
import app.chilpa
import app.chilpa.kit
import "../Theme"

// A delayed command must still refer to the same list, selection and occurrence.
ConfirmationDialog {
    id: dialog
    property var target: null
    property int row: -1
    property string action: ""
    property string selection: ""
    property bool grouped: false
    property string groupLabel: ""
    function stamp() {
        if (!target) return ""
        return JSON.stringify([target.kind,target.albumId,target.key,target.order,target.ascending,
            target.count,target.filterRev,target.selectedCount,Player.queueRev,ListenList.revision,target.chosenTrackIdsJson(row)])
    }
    function show(model, at, command, group, label) {
        target = model
        row = at
        action = command
        grouped = group === true
        groupLabel = label || ""
        selection = stamp()
        open()
    }
    function forget() {
        target = null
        selection = ""
        groupLabel = ""
        action = ""
        row = -1
        grouped = false
    }
    targetValid: target !== null && selection === stamp()
    checkTarget: () => target !== null && selection === stamp()
    destructive: true
    title: action === "remove" ? I18n.tr("Remove from playlist", I18n.rev)
        : action.startsWith("buffer:replace:") ? I18n.tr("Replace queue?", I18n.rev) : I18n.tr("Remove from queue", I18n.rev)
    message: action === "remove" ? I18n.tr("The selected tracks will be removed from this playlist and its saved order.", I18n.rev)
        : action.startsWith("buffer:replace:") ? I18n.tr("The destination queue and its saved order will be replaced with the selected tracks.", I18n.rev)
        : action === "listen:after" ? I18n.tr("All tracks after this occurrence will be removed from the queue.", I18n.rev)
        : I18n.tr("This occurrence will be removed from the queue.", I18n.rev)
    confirmText: action.startsWith("buffer:replace:") ? I18n.tr("Replace", I18n.rev) : I18n.tr("Remove", I18n.rev)
    onConfirmed: {
        if (action === "remove") target.removeChosen(row)
        else if (action === "dequeue") Player.removeQueued(row)
        else if (grouped) target.groupAction(row,action,groupLabel)
        else target.listAction(action.startsWith("listen:") ? action.slice(7) : action,row,-1)
        ListenList.refresh()
        forget()
    }
    onDismissed: forget()
}
