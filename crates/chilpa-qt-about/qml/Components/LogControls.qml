import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The level, the search, Refresh and Auto-tail: what the log viewer's
// disclosure reveals first (QBZ's controls row).
Item {
    id: controls
    property bool autoTail: false
    readonly property string level: levelSelect.value || "all"
    readonly property string search: searchInput.text
    signal changed()
    signal refreshAsked()
    signal autoTailAsked(bool on)
    function reset() { levelSelect.value = "all"; searchInput.text = "" }
    height: Metrics.button32 + Metrics.gap2 // derived: QBZ's 34

    SelectField {
        id: levelSelect
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.selectWidth / 2 // derived: compact severity selector
        options: [I18n.tr("All levels", I18n.rev), I18n.tr("Error", I18n.rev), I18n.tr("Warning", I18n.rev), I18n.tr("Info", I18n.rev), I18n.tr("Debug", I18n.rev), I18n.tr("Trace", I18n.rev)]
        values: ["all", "error", "warn", "info", "debug", "trace"]
        value: "all"
        onPicked: value => { levelSelect.value = value; controls.changed() }
    }
    Rectangle {
        anchors.left: levelSelect.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: refreshButton.left
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        height: parent.height
        radius: Metrics.radiusControl
        border.width: Metrics.hairline
        border.color: searchInput.activeFocus ? Theme.color.accent : Theme.color.strokeHairline
        color: Theme.color.fillField
        Icon {
            id: searchGlyph
            anchors.left: parent.left
            anchors.leftMargin: Metrics.gap10
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.search
            size: Metrics.iconSm
            color: Theme.color.textTertiary
        }
        TextInput {
            id: searchInput
            objectName: "developerLogSearch"
            anchors.left: searchGlyph.right
            anchors.leftMargin: Metrics.gap8
            anchors.right: parent.right
            anchors.rightMargin: Metrics.gap10
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.color.textPrimary
            font.pointSize: Type.body
            clip: true
            selectByMouse: true
            onTextEdited: controls.changed()
        }
        Text {
            visible: searchInput.text === ""
            anchors.fill: searchInput
            verticalAlignment: Text.AlignVCenter
            text: I18n.tr("Search logs", I18n.rev)
            color: Theme.color.textTertiary
            font.pointSize: Type.body
            elide: Text.ElideRight
        }
    }
    LogActionButton {
        id: refreshButton
        objectName: "developerRefreshLogs"
        anchors.right: tail.left
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Refresh", I18n.rev)
        glyph: Icons.refresh
        onClicked: controls.refreshAsked()
    }
    Row {
        id: tail
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Auto-tail", I18n.rev); font.pointSize: Type.body; color: Theme.color.textSecondary }
        Toggle { objectName: "developerAutoTail"; anchors.verticalCenter: parent.verticalCenter; checked: controls.autoTail; onToggled: on => controls.autoTailAsked(on) }
    }
}
