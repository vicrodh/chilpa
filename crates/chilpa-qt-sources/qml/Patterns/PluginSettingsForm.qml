import "qrc:/qt/qml/app/chilpa/wizard/qml/Components"
import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"

Column {
    id: form
    required property var document
    property string pluginName: ""
    property string pluginId: ""
    property string hostStatus: ""
    property bool hostBusy: false
    property var shownFields: []
    property string fieldScope: ""
    property var values: ({})
    signal submitted(string action, var values, var scope)
    // Whether a sign-in window can open a link that declares a return address.
    property bool captures: false
    signal linked(string url)
    signal captured(var link)
    signal refreshed()
    property bool wasWaiting: false
    property double refreshDeadline: 0
    Timer {
        interval: Math.max(1000, Math.min(5000, Number((form.document.wizard || {}).refresh_ms) || 2000))
        running: form.visible && (!form.Window.window || (form.Window.window.visible && form.Window.window.visibility !== Window.Minimized))
            && form.enabled && !form.hostBusy && (form.document.wizard || {}).waiting === true
            && Number((form.document.wizard || {}).refresh_ms) > 0 && Date.now() < form.refreshDeadline
        repeat: true
        onTriggered: form.refreshed()
    }
    spacing: Metrics.gap12
    onDocumentChanged: {
        values = visible ? Object.assign({}, document.values || {}) : ({})
        updateFields()
        const waiting = (document.wizard || {}).waiting === true
        if (wasWaiting && !waiting && visible && Window.window) Window.window.requestActivate()
        if (waiting && !wasWaiting) refreshDeadline = Date.now() + 600000 // bounded: ten-minute sign-in window
        wasWaiting = waiting
    }
    onVisibleChanged: {
        if (visible) { values = Object.assign({}, (document || {}).values || {}); updateFields() }
        else { shownFields = []; fieldScope = ""; values = ({}) }
    }
    onPluginIdChanged: { shownFields = []; fieldScope = ""; updateFields() }
    function updateFields() {
        const doc = document || {}, wizard = doc.wizard || {}
        const scope = JSON.stringify([pluginId,doc.page || "",wizard.step ?? ""])
        const fields = (doc.form || {}).fields || []
        const waiting = wizard.waiting === true || (doc.status || {}).phase === "running"
        if (!visible) { shownFields = []; return }
        if (scope !== fieldScope || !waiting || fields.length > 0) shownFields = fields
        fieldScope = scope
    }
    readonly property string statusMessage: {
        const doc = document || {}
        return [hostStatus,(doc.wizard || {}).description,(doc.status || {}).description]
            .concat(doc.notices || [],(doc.content || []).map(item => item.text))
            .filter(text => typeof text === "string" && text !== "").join("\n\n")
    }

    function actionStamp(action) {
        return action ? JSON.stringify([action.key,action.label,action.fields,action.destructive]) : ""
    }
    function submitAction(action) {
        const payload = {}
        const scope = {plugin:pluginId,revision:document.revision}
        for (const key of action.fields)
            if (values[key] !== undefined) payload[key] = values[key]
        if (action.destructive) {
            confirmation.actionKey = action.key
            confirmation.actionLabel = action.label
            confirmation.effect = String(action.destructive)
            confirmation.scope = scope
            confirmation.definition = actionStamp(action)
            confirmation.payload = payload
            confirmation.open()
        } else dispatch(action.key,payload,scope)
    }
    function dispatch(action,payload,scope) {
        values = ({})
        submitted(action,payload,scope)
    }

    Loader {
        width: form.width
        active: Array.isArray((form.document.wizard || {}).steps) && form.document.wizard.steps.length > 0
        visible: active
        sourceComponent: Component {
            PluginSetupWizard { width: form.width; document: form.document.wizard || ({}); showDescription: false }
        }
    }
    PluginValidation {
        objectName: "pluginValidationArea"
        width: form.width
        inspection: false
        phase: form.hostBusy ? "running" : (form.document.status || {}).phase || "pending"
        label: (form.document.status || {}).label || ""
        problem: form.statusMessage
        progress: (form.document.progress || {}).total > 0
            ? ((form.document.progress || {}).done || 0) / form.document.progress.total : -1
    }

    Repeater {
        model: form.shownFields
        FormField {
            width: form.width
            enabled: ((form.document.form || {}).fields || []).length > 0
            value: form.values[modelData.key]
            onEdited: (value) => { form.values[modelData.key] = value }
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
                objectName: "pluginAction_" + modelData.key
                maximumWidth: form.width
                wrapText: true
                kind: Array.isArray((form.document.wizard || {}).steps) && index === 0 ? "primary" : "secondary"
                onClicked: form.submitAction(modelData)
            }
        }
    }
    ConfirmationDialog {
        id: confirmation
        objectName: "pluginActionConfirmation"
        property string actionKey: ""
        property string actionLabel: ""
        property string effect: ""
        property var scope: ({})
        property string definition: ""
        property var payload: ({})
        title: I18n.trArgs("Confirm “{}” for {}?",[actionLabel,form.pluginName || I18n.tr("Plugin",I18n.rev)],I18n.rev)
        message: effect === "account" ? I18n.tr("This removes the plugin's saved account session and account-related data from this device. You will need to sign in again.",I18n.rev)
            : effect === "files" ? I18n.tr("This permanently deletes files managed by the plugin. Review the affected files in the plugin's description.",I18n.rev)
            : effect === "settings" ? I18n.tr("This replaces the plugin's saved settings. Review the affected settings before proceeding.",I18n.rev)
            : effect === "cache" ? I18n.tr("This permanently clears cached plugin data. Content may need to be downloaded again.",I18n.rev)
            : I18n.tr("This plugin action permanently removes or replaces data. Review the action and its description before proceeding.",I18n.rev)
        destructive: true
        doubleConfirmation: effect !== "settings"
        confirmText: actionLabel
        targetValid: scope.plugin === form.pluginId && String(scope.revision ?? "") === String(form.document.revision ?? "")
            && definition === form.actionStamp(((form.document.form || {}).actions || []).find(action => action.key === actionKey))
        onConfirmed: {
            const values = payload, captured = scope
            payload = ({}); scope = ({})
            form.dispatch(actionKey,values,captured)
        }
        onDismissed: { payload = ({}); scope = ({}) }
    }
}
