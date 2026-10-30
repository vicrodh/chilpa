import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Column {
    id: form
    required property var document
    property var values: ({})
    signal submitted(string action, var values)
    // Whether a sign-in window can open a link that declares a return address.
    property bool captures: false
    signal linked(string url)
    signal captured(var link)
    spacing: Metrics.gap12
    onDocumentChanged: values = Object.assign({}, document.values || {})

    Loader {
        width: form.width
        active: Array.isArray((form.document.wizard || {}).steps) && form.document.wizard.steps.length > 0
        visible: active
        sourceComponent: Component {
            PluginSetupWizard { width: form.width; document: form.document.wizard || ({}) }
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
    PluginValidation {
        width: form.width
        visible: !!form.document.status
            && !Array.isArray((form.document.wizard || {}).steps)
        phase: (form.document.status || {}).phase || "pending"
        label: (form.document.status || {}).label || ""
        problem: (form.document.status || {}).description || ""
    }

    Rectangle {
        width: form.width
        height: Metrics.progress
        radius: Metrics.radiusControl
        color: Theme.color.strokeHairline
        visible: (form.document.progress || {}).total > 0
        Rectangle {
            width: parent.width * Math.max(0, Math.min(1,
                ((form.document.progress || {}).done || 0) / ((form.document.progress || {}).total || 1)))
            height: parent.height
            radius: parent.radius
            color: Theme.color.accent
        }
    }

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
                    objectName: "pluginField_" + field.modelData.key
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
            kind: modelData.primary === true ? "primary" : "secondary"
            glyph: Icons.arrowUpRight
            onClicked: modelData.capture && form.captures ? form.captured(modelData) : form.linked(modelData.url)
        }
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        Repeater {
            model: (form.document.form || {}).actions || []
            PillButton {
                required property var modelData
                required property int index
                text: modelData.label
                kind: Array.isArray((form.document.wizard || {}).steps) && index === 0 ? "primary" : "secondary"
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
