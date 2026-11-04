import QtQuick
import QtQuick.Window
import "../Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components" as Shared
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
Column {
    width: Metrics.settingsMaxWidth
    spacing: Metrics.gap16
    Text { text: "Approved display presentation and shared player controls"; color: Theme.color.textPrimary; font.pointSize: Type.body }
    Shared.AlbumCardActions { width: Metrics.albumCardWidth; height: Metrics.albumCardWidth; playEnabled: false; selectionVisible: false }
    Shared.MenuCascade { id: sharedMenu; entries: [{text:"Display settings",act:"settings"}] }
    PillButton { text: "Shared display menu"; onClicked: sharedMenu.open() }
    Item {
        width: parent.width; height: Metrics.playerHeight
        PluginViewHost { anchors.fill: parent; window: parent; registerHost: false }
        DisplayPlayerIntegration { bar: parent; registerTarget: false }
        IconButton { anchors.right: parent.right; glyph: Icons.pictureInPicture; tip: "View"; onClicked: menu.show() }
        PlayerViewMenu { id: menu; x: parent.width-width }
    }
}
