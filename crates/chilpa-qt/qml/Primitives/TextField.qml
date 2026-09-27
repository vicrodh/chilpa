import QtQuick
import "../Theme"

// A one-line input (design/03 TextField): a placeholder while empty, and,
// for a secret, dots instead of what is typed.
Rectangle {
    id: root

    property alias text: input.text
    property string placeholder
    property bool secret: false
    // True while the field has the keyboard, so a value that changes
    // underneath is not written over what is being typed.
    readonly property alias editing: input.activeFocus

    signal accepted()
    // Enter, or the focus leaving the field.
    signal editingFinished()

    function focusInput() {
        input.forceActiveFocus()
        input.selectAll()
    }

    implicitWidth: Metrics.selectWidth
    implicitHeight: Metrics.fieldHeight
    radius: Metrics.radiusControl
    color: Theme.color.fillField
    border.width: input.activeFocus ? Metrics.focusRing : 0
    border.color: Theme.color.accent

    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: Metrics.fieldPadding
        anchors.rightMargin: Metrics.fieldPadding
        verticalAlignment: TextInput.AlignVCenter
        clip: true
        echoMode: root.secret ? TextInput.Password : TextInput.Normal
        font.pointSize: Type.body
        color: Theme.color.textPrimary
        selectionColor: Theme.color.rowSelected
        onAccepted: root.accepted()
        onEditingFinished: root.editingFinished()
        // A long value set from outside shows its start, not its end.
        onTextChanged: if (!activeFocus) cursorPosition = 0
        onActiveFocusChanged: if (!activeFocus) cursorPosition = 0

        Text {
            anchors.fill: parent
            verticalAlignment: Text.AlignVCenter
            visible: input.text === ""
            text: root.placeholder
            font: input.font
            color: Theme.color.textTertiary
            elide: Text.ElideRight
        }
    }
}
