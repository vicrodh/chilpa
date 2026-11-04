import "../Components" as Presentation
import QtQuick
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Trusted interfaces have the host's privileges; the approval names that boundary.
Item {
    id: page
    required property string pluginId
    required property url entry
    Text {
        anchors.centerIn: parent
        width: parent.width - Metrics.contentHorizontalMargins
        visible: Presentation.DisplayMode.loadFailed
        text: I18n.tr("The plugin interface could not be opened.", I18n.rev)
        color: Theme.color.textSecondary
        font.pointSize: Type.body
        wrapMode: Text.WordWrap
        horizontalAlignment: Text.AlignHCenter
    }
    Component.onCompleted: Presentation.DisplayMode.mount(page)
    Component.onDestruction: Presentation.DisplayMode.unmount(page)
}
