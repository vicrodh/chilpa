import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Each plugin owns a QBZ-style scene result. A failed catalog remains
// selectable so its error is visible instead of shortening another result.
Row {
    id: tabs
    property var sources: []
    property string currentId: ""
    signal selected(string id)
    height: Metrics.sceneNavHeight
    spacing: Metrics.gap8

    Repeater {
        model: tabs.sources
        PillButton {
            required property var modelData
            kind: modelData.id === tabs.currentId ? "primary" : "secondary"
            text: modelData.name + " · " + (modelData.state === "error"
                ? I18n.tr("Unavailable", I18n.rev) : String((modelData.artists || []).length))
            onClicked: tabs.selected(modelData.id)
        }
    }
}
