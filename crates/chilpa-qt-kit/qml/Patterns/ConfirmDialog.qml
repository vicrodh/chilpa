import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// A question before something that cannot be taken back: what will happen,
// in a sentence or two, and the two ways out. The confirming button says
// what it does, never "OK".
Modal {
    id: dialog

    property string title
    property string message
    property string confirmText
    property string cancelText: I18n.tr("Cancel", I18n.rev)
    // The confirming button in the accent, or as the other one when what it
    // does loses something.
    property bool destructive: false

    signal confirmed()
    // Cancel, Esc or a click outside: every way out that is not confirming.
    signal dismissed()

    property bool answered: false
    onOpened: answered = false
    onClosed: if (!answered) dismissed()

    width: Metrics.confirmWidth

    Column {
        width: parent.width
        spacing: Metrics.gap12

        Text {
            width: parent.width
            text: dialog.title
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
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { text: dialog.cancelText; onClicked: dialog.close() }
            PillButton {
                kind: dialog.destructive ? "secondary" : "primary"
                text: dialog.confirmText
                onClicked: {
                    dialog.answered = true
                    dialog.close()
                    dialog.confirmed()
                }
            }
        }
    }
}
