import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The tabs of a source's page: each one is a listing of its own, opened in
// its place. With a width they wrap; without one they stay on a single line.
Flow {
    id: tabs

    property var model: []
    property var navigate: null
    property string opened: ""

    spacing: Metrics.gap8
    visible: model.length > 0
    height: visible ? implicitHeight : 0

    Repeater {
        model: tabs.model
        PillButton {
            required property var modelData
            kind: (tabs.opened ? tabs.opened === modelData.key : modelData.active) ? "primary" : "secondary"
            text: modelData.title
            onClicked: if ((modelData.pages || []).length > 0) tabs.opened = tabs.opened === modelData.key ? "" : modelData.key; else if (modelData.active) tabs.opened = ""; else { tabs.opened = ""; tabs.navigate ? tabs.navigate(modelData.key) : Navigation.goKey("source", modelData.key) }
        }
    }
}
