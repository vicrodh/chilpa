import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

Column {
    id: form
    required property var document
    property var values: ({})
    signal submitted(string action, var values)
    signal linked(string url)
    spacing: Metrics.gap12
    onDocumentChanged: values = Object.assign({}, document.values || {})

    Repeater {
        model: form.document.content || []
        Text {
            required property var modelData
            width: form.width
            text: modelData.text
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
    }
    Repeater {
        model: form.document.notices || []
        Text {
            required property string modelData
            width: form.width
            text: modelData
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.color.statusWarning
            font.pointSize: Type.body
        }
    }
    Repeater {
        model: (form.document.form || {}).fields || []
        Column {
            id: field
            required property var modelData
            width: form.width
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
                sourceComponent: field.modelData.control.type === "toggle" ? toggle : field.modelData.control.type === "select" ? select : input
            }
            Component {
                id: input
                TextField {
                    secret: field.modelData.control.type === "secret"
                    text: String(form.values[field.modelData.key] ?? "")
                    onTextEdited: form.values[field.modelData.key] = field.modelData.control.type === "number" ? Number(text) : text
                }
            }
            Component {
                id: toggle
                Toggle {
                    objectName: "pluginToggle"
                    checked: form.values[field.modelData.key] === true
                    onToggled: (on) => { checked = on; form.values[field.modelData.key] = on }
                }
            }
            Component {
                id: select
                SelectField {
                    objectName: "pluginSelect"
                    width: parent ? parent.width : implicitWidth
                    options: field.modelData.control.choices.map(choice => choice.label)
                    values: field.modelData.control.choices.map(choice => String(choice.value))
                    value: String(form.values[field.modelData.key] ?? "")
                    onPicked: (selected) => { value = selected; form.values[field.modelData.key] = selected }
                }
            }
        }
    }
    Repeater {
        model: form.document.links || []
        PillButton {
            required property var modelData
            text: modelData.label || modelData.url
            onClicked: form.linked(modelData.url)
        }
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        Repeater {
            model: (form.document.form || {}).actions || []
            PillButton {
                required property var modelData
                text: modelData.label
                onClicked: {
                    const submitted = {}
                    for (const key of modelData.fields)
                        if (form.values[key] !== undefined) submitted[key] = form.values[key]
                    form.submitted(modelData.key, submitted)
                    form.values = ({})
                }
            }
        }
    }
}
