import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Words of a title (F3): type them and press Enter.
Rectangle {
    id: words
    property string field
    signal edit(var change)
    implicitHeight: Refine.wordsTall
    radius: Refine.valueRound
    color: Theme.color.fillField
    TextInput {
        id: input
        objectName: "pickWords"
        anchors.fill: parent
        anchors.leftMargin: Refine.addPad
        anchors.rightMargin: Refine.addPad
        verticalAlignment: TextInput.AlignVCenter
        clip: true
        font.pointSize: Type.body
        color: Theme.color.textPrimary
        Keys.onReturnPressed: { if (input.text.trim() !== "") words.edit({ op: "words", field: words.field, word: input.text.trim() }); input.text = "" }
        Keys.onEnterPressed: { if (input.text.trim() !== "") words.edit({ op: "words", field: words.field, word: input.text.trim() }); input.text = "" }
        Text { anchors.fill: parent; verticalAlignment: Text.AlignVCenter; visible: input.text === ""; text: I18n.tr("Type words and press Enter", I18n.rev); font: input.font; color: Theme.color.textTertiary }
    }
    function focusInput() { input.forceActiveFocus() }
}
