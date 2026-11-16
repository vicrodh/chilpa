import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// Two explicit decisions precede a destructive command. Reopening starts over.
Column {
    id: confirmation
    property int step: 1
    readonly property bool ready: step === 1 || acknowledgement.checked
    spacing: Metrics.gap8

    function reset() {
        step = 1
        acknowledgement.checked = false
    }
    function advance() {
        if (step === 1) {
            step = 2
            return false
        }
        return acknowledgement.checked
    }

    Text {
        objectName: "destructiveConfirmationStep"
        width: parent.width
        text: I18n.trArgs("Confirmation {} of {}", [confirmation.step, 2], I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.meta
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }
    Text {
        objectName: "destructiveConfirmationWarning"
        visible: confirmation.step === 2
        width: parent.width
        text: I18n.tr("Check the selected item and the changes described above. This action cannot be undone from Chilpa.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.statusNegative
    }
    CircleCheck {
        id: acknowledgement
        objectName: "destructiveAcknowledgement"
        visible: confirmation.step === 2
        width: parent.width
        text: I18n.tr("I have reviewed the changes and want to continue.", I18n.rev)
    }
}
