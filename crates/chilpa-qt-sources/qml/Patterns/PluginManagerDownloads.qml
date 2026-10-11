import QtQuick
import app.chilpa

PluginDownloads {
    id: pane
    required property var controller
    visible: controller.downloadsOpen
    document: controller.view.downloads_id === controller.downloadsId ? controller.view.downloads || ({}) : ({})
    plugin: controller.installedRows.find(p => p.id === controller.downloadsId) || ({})
    purchaseCard: controller.purchaseCard
    busy: controller.view.busy === true
    problem: controller.view.error || ""
    onBack: { if (controller.initialDownloadsId !== "") Navigation.back(); else controller.downloadsId = "" }
    onAccount: controller.settings(plugin)
    onChooseFolder: controller.command("choose_export_folder", {id: controller.downloadsId})
    onAction: request => controller.downloadAction(request)
}
