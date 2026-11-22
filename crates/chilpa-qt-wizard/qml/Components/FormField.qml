import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: field
    required property var modelData
    property var value
    signal edited(var value)
    spacing: Metrics.gap6
    Text {
        text: field.modelData.label
        textFormat: Text.PlainText
        color: Theme.color.textPrimary
        font.pointSize: Type.body
        width: parent.width
        wrapMode: Text.WordWrap
    }
    Loader {
        width: field.modelData.control.type === "toggle" ? Metrics.toggleWidth : parent.width
        sourceComponent: field.modelData.control.type === "toggle" ? toggle : field.modelData.control.type === "select" ? select : field.modelData.control.type === "path" ? pathPicker : field.modelData.control.type === "copy" ? copyValue : input
    }
    Component {
        id: pathPicker
        PathField {
            objectName: "pluginPath_" + field.modelData.key
            value: String(field.value ?? "")
            directory: field.modelData.control.directory === true
            onPicked: (path) => { value = path; field.edited(path) }
        }
    }
    Component {
        id: copyValue
        CopyField { value: String(field.value ?? "") }
    }
    Component {
        id: input
        TextField {
            objectName: "pluginField_" + field.modelData.key
            secret: field.modelData.control.type === "secret"
            text: String(field.value ?? "")
            onTextEdited: field.edited(field.modelData.control.type === "number" ? Number(text) : text)
        }
    }
    Component {
        id: toggle
        Toggle {
            objectName: "pluginToggle"
            checked: field.value === true
            onToggled: (on) => { checked = on; field.edited(on) }
        }
    }
    Component {
        id: select
        SelectField {
            objectName: "pluginSelect"
            width: parent ? parent.width : implicitWidth
            options: field.modelData.control.choices.map(choice => choice.label)
            values: field.modelData.control.choices.map(choice => String(choice.value))
            value: String(field.value ?? "")
            onPicked: (selected) => { value = selected; field.edited(selected) }
        }
    }
}
