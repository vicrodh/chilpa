import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Replaces a text in one field of the record, in one step: the record's own
// field, or that field on every track. What changes shows in the form,
// unsaved, like any edit. `fields` is the editor's `replaceable`.
Modal {
    id: dialog
    objectName: "metadataReplaceDialog"

    property var fields: []
    property string field: ""
    property bool whole: false
    property bool matchCase: false

    signal replaceAsked(string field, string find, string replacement, bool whole, bool matchCase)

    preferredWidth: Metrics.confirmWidth
    onOpened: {
        if (!fields.some(row => row.key === field))
            field = fields.length > 0 ? fields[0].key : ""
        find.forceActiveFocus()
    }

    function submit() {
        if (find.text === "" || field === "")
            return
        dialog.close()
        dialog.replaceAsked(field, find.text, replacement.text, whole, matchCase)
    }

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width; wrapMode: Text.WordWrap; textFormat: Text.PlainText
            text: I18n.tr("Replace text in a field", I18n.rev)
            font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary
        }
        SelectField {
            objectName: "metadataReplaceField"
            width: parent.width
            values: dialog.fields.map(row => row.key)
            options: dialog.fields.map(row => row.label)
            value: dialog.field
            onPicked: value => dialog.field = value
        }
        TextField {
            id: find
            objectName: "metadataReplaceFind"
            width: parent.width
            placeholder: I18n.tr("Find", I18n.rev)
            onAccepted: dialog.submit()
        }
        TextField {
            id: replacement
            objectName: "metadataReplaceWith"
            width: parent.width
            placeholder: I18n.tr("Replace with", I18n.rev)
            onAccepted: dialog.submit()
        }
        Row {
            spacing: Metrics.gap6
            Toggle { objectName: "metadataReplaceWhole"; anchors.verticalCenter: parent.verticalCenter; checked: dialog.whole; onToggled: on => dialog.whole = on }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Only the whole value", I18n.rev)
                font.pointSize: Type.meta; color: Theme.color.textSecondary
            }
        }
        Row {
            spacing: Metrics.gap6
            Toggle { objectName: "metadataReplaceCase"; anchors.verticalCenter: parent.verticalCenter; checked: dialog.matchCase; onToggled: on => dialog.matchCase = on }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Match case", I18n.rev)
                font.pointSize: Type.meta; color: Theme.color.textSecondary
            }
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
            PillButton {
                objectName: "metadataReplaceApply"
                kind: "primary"
                text: I18n.tr("Replace", I18n.rev)
                enabled: find.text !== "" && dialog.field !== ""
                onClicked: dialog.submit()
            }
        }
    }
}
