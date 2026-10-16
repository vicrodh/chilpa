import QtQuick
import "../Theme"

// A field whose text is hidden as it is typed, and the button that sends
// it (design/03 SettingsRow, secret): a token.
Row {
    id: field

    property string placeholder
    property string action

    signal sent(string text)

    spacing: Metrics.gap8

    TextField {
        id: input
        anchors.verticalCenter: parent.verticalCenter
        secret: true
        placeholder: field.placeholder
        onAccepted: if (text.trim() !== "") send.clicked()
    }
    PillButton {
        id: send
        anchors.verticalCenter: parent.verticalCenter
        text: field.action
        enabled: input.text.trim() !== ""
        onClicked: field.sent(input.text)
    }
}
