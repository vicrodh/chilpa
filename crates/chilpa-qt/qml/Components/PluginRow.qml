import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

Item {
    id: row
    required property var plugin
    property bool installed: false
    signal inspected()
    signal switched(bool on)
    signal settings()
    signal moved(int position)
    height: Math.max(Metrics.settingsRow, identity.implicitHeight + Metrics.gap16)
    Column {
        id: identity
        anchors.left: parent.left
        anchors.right: controls.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4
        Text {
            width: parent.width
            text: row.plugin.name || ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.color.textPrimary
            font.pointSize: Type.body
        }
        Text {
            objectName: "pluginHelp"
            width: parent.width
            text: [row.plugin.version ? I18n.trArgs("Version: {}", [row.plugin.version], I18n.rev) : "", row.plugin.description || ""].filter(Boolean).join(" · ")
            visible: text !== ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.color.textSecondary
            font.pointSize: Type.meta
        }
    }
    Row {
        id: controls
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap12
        IconButton {
            objectName: "pluginMoveUp"
            visible: row.installed && row.plugin.sidebar_position !== null && row.plugin.sidebar_position !== undefined
            glyph: Icons.chevronUp
            tip: I18n.tr("Move up", I18n.rev)
            enabled: row.plugin.sidebar_position > 0
            onClicked: row.moved(row.plugin.sidebar_position - 1)
        }
        IconButton {
            objectName: "pluginMoveDown"
            visible: row.installed && row.plugin.sidebar_position !== null && row.plugin.sidebar_position !== undefined
            glyph: Icons.chevronDown
            tip: I18n.tr("Move down", I18n.rev)
            enabled: row.plugin.sidebar_position < row.plugin.sidebar_count - 1
            onClicked: row.moved(row.plugin.sidebar_position + 1)
        }
        Rectangle {
            visible: row.plugin.light !== "native"
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.gap12
            height: width
            radius: width / 2 // derived: circular compatibility indicator
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.ArrowCursor
                hoverEnabled: true
                onContainsMouseChanged: containsMouse ? TipState.show(row.plugin.light === "unknown" ? I18n.tr("Not inspected", I18n.rev) : row.plugin.light === "green" ? I18n.tr("Plugin initialized successfully", I18n.rev) : row.plugin.light === "red" ? row.plugin.problem || I18n.tr("Not compatible", I18n.rev) : I18n.tr("Compatibility is not fully verified", I18n.rev), parent) : TipState.hide(parent)
            }
            objectName: "pluginCompatibilityLight"
            color: row.plugin.light === "red" ? Theme.color.statusNegative : row.plugin.light === "green" ? Theme.color.statusPositive : row.plugin.light === "yellow" ? Theme.color.statusWarning : Theme.color.textTertiary
        }
        PillButton {
            visible: !row.installed
            text: I18n.tr("Inspect", I18n.rev)
            onClicked: row.inspected()
        }
        Toggle {
            visible: row.installed
            anchors.verticalCenter: parent.verticalCenter
            objectName: "pluginEnabledToggle"
            checked: row.plugin.enabled === true
            onToggled: (on) => row.switched(on)
        }
        IconButton {
            glyph: Icons.settings
            objectName: "pluginSettingsButton"
            enabled: row.installed
            opacity: enabled ? 1 : Metrics.disabledOpacity
            tip: I18n.tr("Settings", I18n.rev)
            onClicked: row.settings()
        }
    }
}
