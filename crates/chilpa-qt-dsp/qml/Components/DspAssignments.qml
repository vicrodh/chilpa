import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: assignments
    objectName: "dspAssignments"
    property var state: ({})
    signal command(string action, var data)
    signal findRequested(string query)
    readonly property var scopes: [
        {key: "default", label: I18n.tr("All outputs", I18n.rev)},
        {key: "headphones", label: I18n.tr("Headphones", I18n.rev)},
        {key: "speakers", label: I18n.tr("Speakers", I18n.rev)},
        {key: "bluetooth", label: "Bluetooth"},
        {key: "usb", label: "USB"},
        {key: "other", label: I18n.tr("Other", I18n.rev)}
    ]
    spacing: Metrics.gap24
    Text { width: parent.width; text: assignments.state.output_name || assignments.state.output || ""; font.pointSize: Type.section; color: Theme.color.textPrimary; wrapMode: Text.WordWrap }
    Text {
        width: parent.width
        text: I18n.tr("A device assignment takes priority over its class, then the general default. Classify devices explicitly; Chilpa cannot identify headphones connected to a DAC.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    Row {
        spacing: Metrics.gap12
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Device class", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
        SelectField { objectName: "dspOutputClass"; options: assignments.scopes.slice(1).map(entry => entry.label); values: assignments.scopes.slice(1).map(entry => entry.key); value: assignments.state.output_class || "other"; onPicked: value => assignments.command("classify_output", {class: value}) }
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        PillButton { objectName: "dspAssignDevice"; text: I18n.tr("Assign to this device", I18n.rev); onClicked: assignments.command("assign", {scope: "device"}) }
        PillButton { objectName: "dspInheritAssignment"; text: I18n.tr("Inherit class / default", I18n.rev); enabled: assignments.state.selection_scope === "device"; onClicked: assignments.command("clear_assignment", {}) }
        PillButton { text: I18n.tr("Find profiles for this device", I18n.rev); onClicked: assignments.findRequested(assignments.state.output_name || "") }
    }
    Row {
        spacing: Metrics.gap12
        SelectField { id: scope; objectName: "dspAssignmentScope"; options: assignments.scopes.map(entry => entry.label); values: assignments.scopes.map(entry => entry.key); value: "default"; onPicked: value => scope.value = value }
        PillButton { objectName: "dspAssignRule"; text: I18n.tr("Assign current preset", I18n.rev); onClicked: assignments.command("assign", {scope: scope.value}) }
    }
    Text { width: parent.width; text: I18n.tr("Active assignment", I18n.rev) + ": " + (assignments.scopes.find(entry => entry.key === assignments.state.selection_scope)?.label || (assignments.state.selection_scope === "device" ? I18n.tr("This device", I18n.rev) : I18n.tr("Bypass", I18n.rev))); color: Theme.color.textSecondary; font.pointSize: Type.body; wrapMode: Text.WordWrap }
    Repeater {
        model: assignments.state.assignments || []
        Row {
            id: rule
            required property var modelData
            width: assignments.width
            spacing: Metrics.gap12
            Text { width: parent.width - remove.width - parent.spacing; text: (assignments.scopes.find(entry => entry.key === rule.modelData.scope)?.label || rule.modelData.scope) + " · " + rule.modelData.name; font.pointSize: Type.body; color: Theme.color.textPrimary; elide: Text.ElideRight }
            IconButton { id: remove; glyph: Icons.x; tip: I18n.tr("Remove assignment", I18n.rev); onClicked: assignments.command("clear_rule", {scope: rule.modelData.scope}) }
        }
    }
}
