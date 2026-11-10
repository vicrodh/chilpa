import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// Shared confirmations keep their body only while visible. File deletion,
// integration removal and database cleanup opt into a second confirmation.
Modal {
    id: dialog

    property string title
    property string message
    property string confirmText
    property string cancelText: I18n.tr("Cancel", I18n.rev)
    // The confirming button in the accent, or as the other one when what it
    // does loses something.
    property bool destructive: false
    property bool doubleConfirmation: false
    property bool targetValid: true
    property var checkTarget: null
    property bool invalidated: false
    // A second question the confirmation carries, as a check that starts
    // off every time; empty, none is shown.
    property string option
    property bool optionChecked: false

    signal confirmed()
    // Cancel, Esc or a click outside: every way out that is not confirming.
    signal dismissed()

    property bool answered: false
    onOpened: { answered = false; optionChecked = false; invalidated = false; if (body.item) body.item.reset() }
    onClosed: if (!answered) dismissed()

    preferredWidth: Metrics.confirmWidth

    Loader {
        id: body
        width: parent.width
        active: dialog.visible
        sourceComponent: Column {
            width: body.width
            spacing: Metrics.gap12
            function reset() { steps.reset() }

            Text {
                width: parent.width
                text: dialog.title
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            Text {
                width: parent.width
                text: dialog.message
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
            CircleCheck {
                objectName: "confirmOption"
                visible: dialog.option !== ""
                width: parent.width
                text: dialog.option
                checked: dialog.optionChecked
                onCheckedChanged: dialog.optionChecked = checked
            }
            DestructiveSteps { id: steps; width: parent.width; visible: dialog.doubleConfirmation }
            Text {
                visible: !dialog.targetValid || dialog.invalidated
                width: parent.width
                text: I18n.tr("The target changed. Cancel and review the action again.", I18n.rev)
                wrapMode: Text.WordWrap
                color: Theme.color.statusNegative
                font.pointSize: Type.body
            }
            Flow {
                id: actions
                width: parent.width
                spacing: Metrics.gap8
                layoutDirection: Qt.RightToLeft
                PillButton {
                    objectName: "confirmationAccept"
                    maximumWidth: actions.width
                    wrapText: true
                    kind: dialog.destructive ? "secondary" : "primary"
                    text: dialog.doubleConfirmation && steps.step === 1 ? I18n.tr("Continue", I18n.rev) : dialog.confirmText
                    enabled: dialog.targetValid && !dialog.invalidated && (!dialog.doubleConfirmation || steps.ready)
                    onClicked: {
                        if (!dialog.targetValid) return
                        if (dialog.doubleConfirmation && !steps.advance()) return
                        if (dialog.checkTarget && !dialog.checkTarget()) { dialog.invalidated = true; return }
                        dialog.answered = true
                        dialog.close()
                        dialog.confirmed()
                    }
                }
                PillButton {
                    objectName: "confirmationCancel"
                    maximumWidth: actions.width
                    wrapText: true
                    text: dialog.cancelText
                    onClicked: dialog.close()
                }
            }
        }
    }
}
