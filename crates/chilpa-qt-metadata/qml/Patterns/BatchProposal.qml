import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id:root
    property var plan: ({})
    property bool busy: false
    property double dataRevision: 0
    property var choices: []
    property string genresOperation: "append_unique"
    property string choicePlan: ""
    readonly property bool fieldsMatch: JSON.stringify(choices.filter(choice=>choice.selected && !choice.invalid).map(choice=>choice.field).sort()) === JSON.stringify((plan.fieldChoices || []).filter(choice=>choice.selected && !choice.invalid).map(choice=>choice.field).sort()) && genresOperation === (plan.genresOperation || "append_unique")
    signal fieldsChosen(var fields, string genresOperation)
    signal applied(string id)
    spacing:Metrics.gap8
    onPlanChanged:if (choicePlan !== (plan.id || "")) { choicePlan=plan.id || ""; choices=(plan.fieldChoices || []).map(value=>Object.assign({},value)); genresOperation=plan.genresOperation || "append_unique" }
    Text { width:parent.width; text:root.plan.title || I18n.tr("Review proposal",I18n.rev); textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.section; color:Theme.color.textPrimary }
    Text { width:parent.width; visible:!!root.plan.error; text:root.plan.error || ""; textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.body; color:Theme.color.statusNegative }
    Text { width:parent.width; visible:(root.plan.requiredFields || []).length > 0; text:I18n.tr("This edition requires these identity fields together:",I18n.rev) + " " + (root.plan.requiredFields || []).map(field=>root.choices.find(choice=>choice.field === field)?.label || field).join(", "); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.statusWarning }
    MetadataChanges {
        objectName:"batchCandidateChanges"
        visible:root.plan.kind === "candidate"
        width:parent.width; changes:root.choices; busy:root.busy || root.plan.kind !== "candidate"
        onChoose:(key,selected)=> { root.choices=root.choices.map(choice=>choice.key === key ? Object.assign({},choice,{selected:selected}) : choice) }
    }
    SelectField {
        width:parent.width; visible:root.plan.kind === "candidate" && root.choices.some(choice=>choice.field === "genres" && choice.selected)
        enabled:!root.busy; values:["append_unique","set"]; options:[I18n.tr("Add genres to existing values",I18n.rev),I18n.tr("Replace genres",I18n.rev)];value:root.genresOperation
        onPicked:value=>root.genresOperation=value
    }
    PillButton { objectName:"batchReviewFields"; text:I18n.tr("Preview selected fields",I18n.rev); visible:root.plan.kind === "candidate"; enabled:!root.busy; onClicked:root.fieldsChosen(root.choices.filter(choice=>choice.selected && !choice.invalid).map(choice=>choice.field),root.genresOperation) }
    Repeater {
        model:(root.plan.resultGroups || []).slice(0,32)
        Text { required property var modelData; width:root.width; text:[modelData.title,modelData.artist,I18n.trArgs("{} files",[modelData.tracks || modelData.files || 0],I18n.rev)].filter(Boolean).join(" · "); textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
    }
    Text { width:parent.width; visible:(root.plan.remainingGroups || []).length > 0; text:I18n.tr("Unselected files remain in their current albums.",I18n.rev); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
    Text { width:parent.width; visible:!!root.plan.id && root.plan.baseDataRevision !== root.dataRevision; text:I18n.tr("The selection changed. Preview the proposal again before applying it.",I18n.rev); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.statusWarning }
    PillButton { objectName:"batchApplyPlan"; text:I18n.tr("Add reviewed proposal to pending changes",I18n.rev); enabled:!root.busy && root.plan.canApply === true && root.plan.baseDataRevision === root.dataRevision && (root.plan.kind !== "candidate" || root.fieldsMatch); onClicked:root.applied(root.plan.id) }
    Text { width:parent.width; text:I18n.tr("This adds changes to the file preview. Write reviewed changes saves them.",I18n.rev); wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
}
