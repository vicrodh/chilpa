import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Timer {
    required property var controller
    objectName: "pluginMaintenance"
    interval: Motion.refresh
    repeat: true
    running: controller.visible && controller.detailsView.opened && controller.detailsView.mode === "settings" && controller.detailsView.tab === "settings"
        && controller.selectedPlugin !== null && !!controller.selectedPlugin.origin && controller.selectedPlugin.enabled === true
        && !controller.view.maintenance_failed && !controller.view.working && !controller.view.busy && !controller.filterPending
    onTriggered: controller.command("tick", {id: controller.selected})
}
