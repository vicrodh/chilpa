import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: manager
    property var selection: ({})
    property string presetName: ""
    property bool autosave: false
    signal command(string action, var data)
    spacing: Metrics.gap24
    Text { width: parent.width; text: I18n.tr("Save a copy of the current sound, or rename and replace a personal preset. Factory corrections remain available.", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
    TextField { id: saveName; objectName: "dspSaveName"; width: parent.width; placeholder: manager.presetName || I18n.tr("Name a personal preset", I18n.rev); Accessible.name: I18n.tr("Preset name", I18n.rev) }
    Flow {
        width: parent.width
        spacing: Metrics.gap12
        PillButton { objectName: "dspSavePreset"; text: I18n.tr("Save copy", I18n.rev); enabled: saveName.text.trim() !== ""; onClicked: manager.command("save", {name: saveName.text}) }
        PillButton { objectName: "dspReplacePreset"; text: I18n.tr("Replace personal preset", I18n.rev); enabled: (manager.selection.preset || "").startsWith("user-") && !!manager.selection.edited; onClicked: manager.command("replace", {}) }
        PillButton { objectName: "dspRenamePreset"; text: I18n.tr("Rename", I18n.rev); enabled: (manager.selection.preset || "").startsWith("user-") && saveName.text.trim() !== ""; onClicked: manager.command("rename", {name: saveName.text}) }
        PillButton { text: I18n.tr("Delete preset", I18n.rev); enabled: (manager.selection.preset || "").startsWith("user-"); onClicked: { removal.presetKey = manager.selection.preset; removal.open() } }
    }
    Row {
        width: parent.width
        spacing: Metrics.gap12
        Toggle { id: autosaveSwitch; objectName: "dspAutoSave"; checked: manager.autosave; onToggled: on => manager.command("autosave", {enabled: on}) }
        Text { width: parent.width - autosaveSwitch.width - parent.spacing; text: I18n.tr("Auto save personal presets after adjustment. Factory presets keep a separate draft.", I18n.rev); wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
    }
    ConfirmationDialog {
        id: removal
        property string presetKey: ""
        title: I18n.tr("Delete preset?", I18n.rev)
        message: I18n.tr("This permanently deletes the selected personal audio preset and its saved settings.", I18n.rev)
        confirmText: I18n.tr("Delete preset", I18n.rev)
        destructive: true
        targetValid: presetKey === manager.selection.preset
        onConfirmed: manager.command("delete", {})
    }
}
