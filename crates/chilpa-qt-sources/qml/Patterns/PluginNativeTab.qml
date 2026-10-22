import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as P
import "../Components"

// The native plugins installed, then what the repositories offer that is
// not installed or is newer, in a single scroll. Installed, an offer waits
// switched off.
Flickable {
    id: tab
    objectName: "nativePluginsTab"
    required property var controller
    readonly property var installed: controller.installedRows.filter(p => !p.origin)
    readonly property var offered: controller.view.native_catalog || []
    contentHeight: column.height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: P.ViewportScrollBar { gutter: -Metrics.gap8 }
    Column {
        id: column
        width: tab.width - Metrics.gap12
        spacing: Metrics.gap8
        Text {
            visible: tab.installed.length > 0
            text: I18n.tr("Installed", I18n.rev)
            font.pointSize: Type.titleSmall
            color: Theme.color.textPrimary
        }
        Repeater {
            objectName: "installedNativePlugins"
            model: tab.installed
            delegate: PluginRow {
                required property var modelData
                width: column.width
                plugin: modelData
                installed: true
                enabled: !tab.controller.view.busy
                onMoved: position => tab.controller.command("sidebar_position", {id: modelData.id, position: position})
                onSettings: tab.controller.settings(modelData)
                onOpened: Navigation.goKey("pluginDownloads", modelData.id)
                onSwitched: (on) => tab.controller.command(on ? "review_permissions" : "disable", {id: modelData.id})
            }
        }
        Text {
            visible: tab.offered.length > 0
            topPadding: tab.installed.length > 0 ? Metrics.gap12 : 0
            text: I18n.tr("Available", I18n.rev)
            font.pointSize: Type.titleSmall
            color: Theme.color.textPrimary
        }
        Repeater {
            model: tab.offered
            delegate: Item {
                required property var modelData
                objectName: "nativeCatalogRow"
                width: column.width
                height: Metrics.settingsRow
                Column {
                    anchors.left: parent.left
                    anchors.right: get.left
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    Row {
                        width: parent.width
                        spacing: Metrics.gap8
                        Text { width: Math.min(implicitWidth, parent.width - (offerThirdParty.visible ? offerThirdParty.width + parent.spacing : 0)); text: modelData.name + "  " + modelData.version; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body }
                        ThirdPartyTag { id: offerThirdParty; anchors.verticalCenter: parent.verticalCenter; visible: modelData.third_party === true }
                    }
                    Text { width: parent.width; text: modelData.problem || modelData.description; elide: Text.ElideRight; color: modelData.problem ? Theme.color.statusWarning : Theme.color.textSecondary; font.pointSize: Type.meta }
                }
                P.PillButton {
                    id: get
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.update ? I18n.tr("Update", I18n.rev) : I18n.tr("Install", I18n.rev)
                    enabled: !tab.controller.view.busy && !modelData.problem
                    onClicked: tab.controller.command("install_native", {repository: modelData.repository_id, id: modelData.id})
                }
            }
        }
    }
}
