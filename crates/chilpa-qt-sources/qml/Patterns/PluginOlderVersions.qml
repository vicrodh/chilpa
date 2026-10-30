import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P
import "../Components"

// The versions of a plugin that are not the latest: the list shows one row
// per plugin, and the rest wait here, each to install from where it is offered.
P.Modal {
    id: dialog
    objectName: "pluginOlderVersions"
    required property var controller
    property var plugin: ({})
    readonly property var versions: plugin.older || []
    width: Math.min(Metrics.confirmWidth + Metrics.modalInsets, parent.width - Metrics.modalInsets)
    height: Math.min(content.implicitHeight + Metrics.modalInsets, parent.height - Metrics.modalInsets)
    Column {
        id: content
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Older versions", I18n.rev) + "  " + (dialog.plugin.name || "")
            elide: Text.ElideRight
            font.pointSize: Type.section
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Repeater {
            model: dialog.versions
            delegate: Item {
                required property var modelData
                objectName: "olderVersionRow"
                width: content.width
                height: Metrics.settingsRow
                Row {
                    anchors.left: parent.left
                    anchors.right: install.left
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Metrics.gap8
                    Text { id: label; text: modelData.version; color: Theme.color.textPrimary; font.pointSize: Type.body }
                    ThirdPartyTag { anchors.verticalCenter: parent.verticalCenter; by: modelData.third_party_by || ""; visible: modelData.third_party === true }
                }
                P.PillButton {
                    id: install
                    objectName: "installOlderVersion"
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Install", I18n.rev)
                    enabled: !dialog.controller.view.busy && !modelData.problem
                    onClicked: { dialog.close(); dialog.controller.command("install_native", {repository: modelData.repository_id, id: modelData.id}) }
                }
            }
        }
        P.PillButton { text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
    }
}
