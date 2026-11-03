import QtQuick
import app.chilpa
import "../Patterns"

Item {
    id: page
    required property string pluginId
    required property Component purchaseCard
    SettingsPage { id: backend; section: "plugins" }
    PluginManager { anchors.fill: parent; page: backend; initialDownloadsId: page.pluginId; purchaseCard: page.purchaseCard }
}
