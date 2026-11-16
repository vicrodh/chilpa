import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: form
    property var fields: []
    property bool busy: false
    property int selected: 0
    property string field: "genres"
    property string operation: "append_unique"
    readonly property var spec: fields.find(row => row.key === field) || ({})
    readonly property var operations: spec.operations || ["keep","set","clear","append_unique","remove_values"]
    signal proposed(var patch)
    spacing: Metrics.gap8
    function operationLabel(value) {
        return ({keep:I18n.tr("Keep current values", I18n.rev),set:I18n.tr("Replace values", I18n.rev),clear:I18n.tr("Clear field", I18n.rev),append_unique:I18n.tr("Add values", I18n.rev),remove_values:I18n.tr("Remove values", I18n.rev)})[value] || value
    }
    Text { width:parent.width; text:I18n.tr("Change only the selected field", I18n.rev); color:Theme.color.textPrimary; font.pointSize:Type.body; wrapMode:Text.Wrap }
    SelectField {
        objectName:"batchField"
        width:parent.width; enabled:!form.busy
        values:form.fields.map(row => row.key); options:form.fields.map(row => row.label || row.key); value:form.field
        onPicked: value => { form.field=value; form.operation=value === "genres" ? "append_unique" : "keep" }
    }
    SelectField {
        objectName:"batchOperation"
        width:parent.width; enabled:!form.busy
        values:form.operations; options:form.operations.map(form.operationLabel); value:form.operation
        onPicked: value => form.operation=value
    }
    MultilineField {
        id: input; objectName:"batchValue"
        width:parent.width; height:Metrics.fieldHeightLarge * 2 // derived: two value lines
        visible:form.spec.type !== "boolean" && !["keep","clear"].includes(form.operation)
        enabled:!form.busy; placeholder:form.spec.type === "list" ? I18n.tr("One value per line", I18n.rev) : I18n.tr("Value", I18n.rev)
    }
    SelectField {
        id: booleanValue
        width:parent.width; enabled:!form.busy
        visible:form.spec.type === "boolean" && form.operation === "set"
        options:[I18n.tr("Yes",I18n.rev),I18n.tr("No",I18n.rev)]; values:["true","false"]; value:"false"
    }
    Text {
        width:parent.width
        text:form.operation === "append_unique" ? I18n.tr("Existing values stay on each file. Equivalent values are not added twice.", I18n.rev) : I18n.tr("Other fields stay unchanged.", I18n.rev)
        wrapMode:Text.Wrap; color:Theme.color.textSecondary; font.pointSize:Type.meta
    }
    PillButton {
        objectName:"batchPropose"
        text:I18n.tr("Preview changes", I18n.rev); kind:"primary"; enabled:!form.busy && form.selected > 0
        onClicked: {
            let value=input.text
            if (form.spec.type === "list") value=value.split(/\r?\n/).map(row => row.trim()).filter(Boolean)
            else if (form.spec.type === "boolean") value=booleanValue.value === "true"
            else if (form.spec.type === "number") value=Number(value)
            form.proposed({field:form.field,operation:form.operation,value:value})
        }
    }
}
