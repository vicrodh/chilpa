import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Column {
    id: navigation
    readonly property var downloadItems: JSON.parse(Sources.snapshot || "{}").download_plugins || []
    Repeater {
        model: navigation.downloadItems.filter(item => item.access === "settings")
        NavItem {
            required property var modelData
            width: navigation.width
            text: modelData.name
            glyph: Icons.download
            onClicked: Navigation.goKey("pluginDownloads", modelData.id)
        }
    }
}
