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
    height: Math.max(Metrics.settingsRow, title.implicitHeight + Metrics.gap16)
    Text {
        id: title
        anchors.left: parent.left
        anchors.right: controls.left
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.plugin.name || ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Theme.color.textPrimary
        font.pointSize: Type.body
    }
    Row {
        id: controls
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap12
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
