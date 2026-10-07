import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The tabs of a source's page that has them (Library: All, Releases,
// Tracks...): each one is a listing of its own, opened in its place.
Row {
    id: tabs

    property var model: []

    spacing: Metrics.gap8
    visible: model.length > 0
    height: visible ? implicitHeight : 0

    Repeater {
        model: tabs.model
        PillButton {
            required property var modelData
            kind: modelData.active ? "primary" : "secondary"
            text: modelData.title
            onClicked: if (!modelData.active) Navigation.goKey("source", modelData.key)
        }
    }
}
