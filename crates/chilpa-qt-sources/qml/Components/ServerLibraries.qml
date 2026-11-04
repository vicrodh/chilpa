import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The libraries a server offers, each with its circle: the ones chosen
// come in. All of them chosen is every library, a new one included.
Column {
    id: libraries
    property var choices: []
    property var chosen: []
    signal picked(string id, string name, bool on)
    spacing: Metrics.gap4

    Repeater {
        model: libraries.choices
        CircleCheck {
            required property var modelData
            required property int index
            objectName: "serverLibrary_" + index
            width: libraries.width
            checkable: false
            checked: libraries.chosen.indexOf(modelData.id) >= 0
            text: modelData.name
            onClicked: libraries.picked(modelData.id, modelData.name, !checked)
        }
    }
    Text {
        width: parent.width
        topPadding: Metrics.gap8
        text: libraries.chosen.length === 0 ? I18n.tr("Nothing chosen yet. Choose at least one library.", I18n.rev)
            : I18n.trArgs("{} of {} chosen", [libraries.chosen.length, libraries.choices.length], I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
