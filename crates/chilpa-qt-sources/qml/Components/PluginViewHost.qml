import QtQuick
import QtQml.Models
import app.chilpa
import app.chilpa.sources
import app.chilpa.mini
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id: host
    required property var window
    property var chrome: []
    property bool registerHost: true
    Instantiator {
        model:host.chrome
        delegate:Binding { required property var modelData; target:modelData; property:"visible"; value:false; when:DisplayMode.active; restoreMode:Binding.RestoreBindingOrValue }
    }
    Item { id:dragLayer; parent:host.window && host.window.contentItem ? host.window.contentItem:host; anchors.fill:parent; z:20 }
    property real standardPlayerHeight: Metrics.playerHeight
    readonly property real playerHeight: DisplayMode.customPlayer ? DisplayMode.playerExtent : standardPlayerHeight
    Rectangle { anchors.fill: parent; visible: DisplayMode.active && (!Theme.ambient || DisplayMode.provider && DisplayMode.provider.ambient === false); color: Theme.color.surfaceWindow }
    Item {
        id: stage
        width: parent.width - (DisplayMode.customPlayer && DisplayMode.playerDock !== "bottom" ? DisplayMode.playerExtent : 0)
        x: DisplayMode.customPlayer && DisplayMode.playerDock === "left" ? DisplayMode.playerExtent : 0
        height: parent.height - (DisplayMode.playerDock === "bottom" ? host.playerHeight + (DisplayMode.customPlayer ? 0 : Metrics.spaceWindow + Metrics.spaceWindow) : 0)
    }
    Loader {
        id: mount
        objectName: "pluginDisplayLoader"
        parent: DisplayMode.active ? stage : DisplayMode.portal
        anchors.fill: parent
        // A surface without the display contract goes while its page is
        // hidden and comes back with it; a display keeps its own.
        active: host.registerHost && DisplayMode.portal !== null && String(DisplayMode.entry) !== ""
            && (DisplayMode.provider !== null || DisplayMode.portal.visible || detached)
        readonly property bool detached: item?.detachedVisible === true
        source: active ? DisplayMode.entry : ""
        asynchronous: true
        onItemChanged: if (!item) DisplayMode.surface = null
        onLoaded: DisplayMode.bind(item)
        onStatusChanged: if (status === Loader.Error) { DisplayMode.loadFailed = true; DisplayMode.requested = false; DisplayMode.surface = null }
    }
    Loader {
        objectName: "pluginPlayerLoader"
        active: host.registerHost && DisplayMode.customPlayer
        sourceComponent: active ? DisplayMode.provider.playerComponent : null
        x: DisplayMode.playerDock === "right" ? parent.width - width : 0
        y: DisplayMode.playerDock === "bottom" ? parent.height - height : 0
        width: DisplayMode.playerDock === "bottom" ? parent.width : DisplayMode.playerExtent
        height: DisplayMode.playerDock === "bottom" ? DisplayMode.playerExtent : parent.height
        onLoaded: { item.transport = Player; item.display = DisplayMode.provider }
    }
    Connections { target: Immersive; function onOpenChanged() { if (Immersive.open && DisplayMode.active) DisplayMode.leave() } }
    Component.onCompleted: if (registerHost) { DisplayMode.window = window; DisplayMode.dragLayer = dragLayer }
    Component.onDestruction: if (DisplayMode.window === window) { DisplayMode.window = null; DisplayMode.dragLayer = null }
}
