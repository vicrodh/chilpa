import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/wizard/qml/Components"

// Removing a server: the question, then, while its songs and playlists
// leave the library away from the window, the dialog stays and says so over
// everything in it, and nothing closes it. Success closes it, and the
// window says so in a notice; a failure stays, readable, with what to do and
// the library's own words to copy.
Modal {
    id: dialog
    property int serverId: -1
    property string message
    property var preview: null
    readonly property var setup: preview || JSON.parse(Servers.setup || "{}")
    readonly property var removal: setup.removing && setup.removing.id === serverId ? setup.removing : ({})
    readonly property bool removing: removal.state === "removing"
    readonly property bool failed: removal.state === "failed"
    readonly property bool asked: removing || failed
    property bool sent: false

    width: Metrics.confirmWidth
    closePolicy: removing || (sent && !failed) ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

    onAboutToShow: sent = false
    onClosed: if (!preview) Servers.cancel()
    // Closed after the change is taken in: closing says so to the facade,
    // which changes the state being read.
    onRemovalChanged: if (sent && removal.state === "done") Qt.callLater(close)

    function remove() {
        sent = true
        Servers.remove(serverId)
        Servers.refresh()
    }

    // How the removal goes, read while it runs.
    Timer {
        interval: Motion.poll
        repeat: true
        running: dialog.visible && !dialog.preview && dialog.sent && !dialog.failed
        onTriggered: Servers.refresh()
    }

    Item {
        width: parent.width
        height: Math.max(question.implicitHeight, busy.implicitHeight + Metrics.gap24 * 2) // derived: the notice with room around it

        Column {
            id: question
            width: parent.width
            spacing: Metrics.gap12
            enabled: !dialog.removing

            Text {
                width: parent.width
                text: I18n.tr("Remove this server?", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Text {
                width: parent.width
                text: dialog.message
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
            WizardBanner {
                objectName: "serverRemoveFailed"
                visible: dialog.failed
                variant: "error"
                title: dialog.removal.title || ""
                body: dialog.removal.advice || ""
            }
            TextEdit {
                objectName: "serverRemoveDetail"
                visible: dialog.failed && (dialog.removal.detail || "") !== ""
                width: parent.width
                text: dialog.removal.detail || ""
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            Row {
                anchors.right: parent.right
                spacing: Metrics.gap8
                PillButton {
                    objectName: "serverRemoveCancel"
                    text: dialog.failed ? I18n.tr("Close", I18n.rev) : I18n.tr("Cancel", I18n.rev)
                    onClicked: dialog.close()
                }
                PillButton {
                    objectName: "serverRemoveConfirm"
                    kind: "secondary"
                    text: dialog.failed ? I18n.tr("Try again", I18n.rev) : I18n.tr("Remove", I18n.rev)
                    onClicked: dialog.remove()
                }
            }
        }

        // Over the whole dialog while its rows leave: what is happening, and
        // no way to press anything under it.
        Rectangle {
            objectName: "serverRemoving"
            anchors.fill: parent
            visible: dialog.removing
            radius: Metrics.radiusControl
            color: Theme.color.overlayScrim
            Hold {}
            Column {
                id: busy
                anchors.centerIn: parent
                width: parent.width - Metrics.gap24 * 2 // derived: inset on both sides
                spacing: Metrics.gap12
                BusyMark {
                    anchors.horizontalCenter: parent.horizontalCenter
                    clock: Pulse
                    active: dialog.visible && dialog.removing
                    size: Metrics.iconMd
                    color: Theme.color.overlayInk
                }
                Text {
                    objectName: "serverRemovingText"
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: I18n.trArgs("Removing {}… Its songs and playlists are leaving your library. This can take a moment.", [dialog.removal.name || ""], I18n.rev)
                    wrapMode: Text.WordWrap
                    font.pointSize: Type.body
                    color: Theme.color.overlayInk
                }
            }
        }
    }
}
