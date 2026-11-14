import QtQuick
import app.chilpa
import "../Theme"
import "../Primitives"

Flow {
    id: line
    objectName: "albumGenreLine"
    property var document: ({})
    property bool contrasted: false
    readonly property var entries: document.items || []
    visible: entries.length > 0
    spacing: Metrics.gap4
    Repeater {
        model: line.entries
        PillButton {
            required property var modelData
            objectName: "albumGenre:" + modelData.name
            maximumWidth: line.width
            text: modelData.name
            kind: line.contrasted ? "backdrop" : "ghost"
            onClicked: Navigation.goKey("genres", modelData.key)
        }
    }
}
