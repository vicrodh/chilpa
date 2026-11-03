import QtQuick
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Trusted interfaces have the host's privileges; the approval names that boundary.
Item {
    id: page
    required property string pluginId
    required property url entry
    Loader {
        id: surface
        objectName: "pluginDisplayLoader"
        anchors.fill: parent
        // A trusted display may move its surface to its own visible window.
        active: String(page.entry) !== "" && (page.visible || item?.detachedVisible === true)
        source: active ? page.entry : ""
        asynchronous: true
    }
    Text {
        anchors.centerIn: parent
        visible: surface.status === Loader.Error
        text: I18n.tr("The plugin interface could not be opened.", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.statusNegative
    }
}
