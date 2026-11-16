import QtQuick
import app.chilpa
import "../Patterns"

Item {
    id: page
    required property string pluginId
    required property Component purchaseCard
    PluginManager { anchors.fill: parent; initialDownloadsId: page.pluginId; purchaseCard: page.purchaseCard }
}
