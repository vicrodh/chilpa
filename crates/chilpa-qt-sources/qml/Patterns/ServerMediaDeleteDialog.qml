import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// Mounted on demand by a music menu. Only confirmation starts network work.
Modal {
    id: dialog
    objectName: "serverMediaDeleteDialog"
    required property string kind
    required property int localId
    property var info: ({})
    property var receipt: ({})
    readonly property bool busy: receipt.pending === true
    preferredWidth: Metrics.confirmWidth
    closePolicy: busy ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnReleaseOutside
    onOpened: { info = JSON.parse(Sources.serverMediaInfo(kind, localId)); receipt = ({}); confirmation.reset() }

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: dialog.kind === "album" ? I18n.tr("Delete album from server?", I18n.rev) : I18n.tr("Delete track from server?", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "serverDeleteTitle"
            width: parent.width
            text: dialog.info.title || ""
            textFormat: Text.PlainText
            maximumLineCount: 3 // bounded: remote metadata cannot grow the dialog without limit
            elide: Text.ElideRight
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: (dialog.info.server || "") + "\n" + (dialog.info.address || "")
            textFormat: Text.PlainText
            maximumLineCount: 2 // bounded: one name and one address
            elide: Text.ElideRight
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            text: dialog.kind === "album"
                ? I18n.tr("This permanently deletes the album and all its tracks from the server's filesystem. It cannot be undone from Chilpa.", I18n.rev)
                : I18n.tr("This permanently deletes the track from the server's filesystem. It cannot be undone from Chilpa.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Text {
            objectName: "serverDeleteStatus"
            width: parent.width
            text: dialog.busy ? I18n.tr("Deleting…", I18n.rev)
                : dialog.receipt.error || (dialog.receipt.done ? I18n.tr("Deleted from server.", I18n.rev)
                : !dialog.info.ref ? I18n.tr("This item is no longer available from the server.", I18n.rev) : "")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: dialog.receipt.error ? Theme.color.statusNegative : Theme.color.textSecondary
        }
        DestructiveSteps {
            id: confirmation
            visible: !dialog.receipt.request_id
            width: parent.width
        }
        Flow {
            id: actions
            width: parent.width
            spacing: Metrics.gap8
            PillButton {
                objectName: "serverDeleteCancel"
                maximumWidth: actions.width
                wrapText: true
                text: dialog.receipt.request_id ? I18n.tr("Close", I18n.rev) : I18n.tr("Cancel", I18n.rev)
                enabled: !dialog.busy
                onClicked: dialog.close()
            }
            PillButton {
                objectName: "serverDeleteConfirm"
                maximumWidth: actions.width
                wrapText: true
                text: confirmation.step === 1 ? I18n.tr("Continue", I18n.rev) : I18n.tr("Delete from server", I18n.rev)
                visible: !dialog.receipt.request_id
                enabled: !!dialog.info.ref && !dialog.busy && confirmation.ready
                onClicked: {
                    if (confirmation.advance())
                        dialog.receipt = JSON.parse(Sources.serverMediaDelete(dialog.kind, dialog.localId, dialog.info.confirmation, dialog.info.source))
                }
            }
        }
    }
    Timer {
        interval: Motion.poll
        repeat: true
        running: dialog.visible && dialog.busy && dialog.contentItem.Window.window && dialog.contentItem.Window.window.visible
        onTriggered: dialog.receipt = JSON.parse(Sources.serverMediaStatus(dialog.receipt.request_id))
    }
}
