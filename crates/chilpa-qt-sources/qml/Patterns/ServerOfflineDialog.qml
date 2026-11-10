import QtQuick
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The job belongs to the facade. Closing the dialog releases its UI.
Modal {
    id: dialog
    objectName: "serverOfflineDialog"
    property string kind: "tracks"
    property string selected: "[]"
    property var info: ({})
    property var status: ({})
    property var removal: ({})
    readonly property bool busy: status.running === true
    preferredWidth: Metrics.confirmWidth
    onOpened: {
        info = JSON.parse(Sources.offline("info", kind, selected))
        refresh()
    }
    function refresh() {
        const working = busy
        status = JSON.parse(Sources.offline("status", "", "[]"))
        if (working && !busy) info = JSON.parse(Sources.offline("info", kind, selected))
    }
    onClosed: Sources.offline("dismiss_removal", "", "")

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Offline music", I18n.rev)
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: dialog.kind === "downloads"
                ? I18n.trArgs("{} offline audio files saved on this device.", [dialog.info.stored_count || 0], I18n.rev)
                : I18n.trArgs("Download {} server tracks for offline listening.", [dialog.info.count || 0], I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            text: I18n.tr("Original audio is saved in the offline folder configured for each server. Offline storage is limited to 2 GiB, with up to 512 MiB per file.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
        StatusArea {
            objectName: "offlineDownloadStatus"
            width: parent.width
            progress: dialog.status.total > 0 ? ((dialog.status.done || 0) + (dialog.status.failed || 0)) / dialog.status.total : -1
            text: (dialog.status.title || "") + "\n"
                + (dialog.busy && dialog.status.phase === "verifying" ? I18n.tr("Verifying downloaded audio…", I18n.rev)
                    : dialog.busy && dialog.status.phase === "checking" ? I18n.tr("Checking downloaded audio…", I18n.rev) : "") + "\n"
                + (dialog.status.total ? (dialog.status.removing
                    ? I18n.trArgs("{} of {} files removed. {} failed.", [dialog.status.done || 0, dialog.status.total, dialog.status.failed || 0], I18n.rev)
                    : I18n.trArgs("{} of {} downloaded. {} failed.", [dialog.status.done || 0, dialog.status.total, dialog.status.failed || 0], I18n.rev)) : "")
                + "\n" + (dialog.status.error || (dialog.status.cancelled ? I18n.tr("Download cancelled.", I18n.rev) : ""))
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton {
                objectName: "startOfflineDownload"
                text: I18n.tr("Download", I18n.rev)
                kind: "primary"
                enabled: !dialog.busy && dialog.info.count > 0 && !dialog.info.too_many
                onClicked: dialog.status = JSON.parse(Sources.offline("start", dialog.kind, dialog.selected))
            }
            PillButton {
                objectName: "cancelOfflineDownload"
                enabled: dialog.busy
                text: I18n.tr("Cancel", I18n.rev)
                onClicked: dialog.status = JSON.parse(Sources.offline("cancel", "", "[]"))
            }
            PillButton {
                objectName: "removeOfflineFiles"
                text: I18n.tr("Remove downloaded files…", I18n.rev)
                enabled: !dialog.busy
                onClicked: {
                    dialog.removal = JSON.parse(Sources.offline("removal_info", dialog.kind, dialog.selected))
                    if (dialog.removal.ticket) confirmation.open()
                    else dialog.status = dialog.removal
                }
            }
            PillButton { text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
    }
    ConfirmationDialog {
        id: confirmation
        objectName: "offlineRemovalConfirmation"
        title: I18n.tr("Remove downloaded audio?", I18n.rev)
        message: I18n.trArgs("This permanently deletes {} downloaded audio files from this device.", [dialog.removal.count || 0], I18n.rev)
        destructive: true
        doubleConfirmation: true
        confirmText: I18n.tr("Remove", I18n.rev)
        onConfirmed: dialog.status = JSON.parse(Sources.offline("remove", "", dialog.removal.ticket))
        onDismissed: Sources.offline("dismiss_removal", "", "")
    }
    Timer {
        interval: 500
        running: dialog.opened && dialog.busy
        repeat: true
        onTriggered: dialog.refresh()
    }
}
