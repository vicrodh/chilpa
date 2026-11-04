pragma Singleton
import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources

// One approved display mount shared by its route and the full-window host.
QtObject {
    id: mode
    property var portal: null
    property var surface: null
    property var window: null
    property var standardTarget: null
    property var dragLayer: null
    property var previousGeometry: null
    property bool requested: false
    property bool loadFailed: false
    property string pendingPlugin: ""
    property string pendingAction: ""
    property bool changedFullscreen: false
    readonly property string pluginId: portal ? portal.pluginId : ""
    readonly property url entry: portal ? Sources.display(pluginId, Sources.revision) : ""
    readonly property var provider: surface && surface.display && surface.display.version === 1 ? surface.display : null
    readonly property bool active: requested && portal !== null && String(entry) !== ""
    readonly property bool customPlayer: active && provider !== null && provider.useCustomPlayer && provider.playerComponent !== null
    readonly property real playerExtent: customPlayer ? provider.playerExtent || 112 : 0
    readonly property string playerDock: customPlayer ? provider.playerDock || "bottom" : "bottom"
    readonly property var trayTarget: customPlayer && provider.trayTarget ? provider.trayTarget : standardTarget
    // Read when the view menu opens, not on every change of the sources' snapshot.
    function panels() { return (JSON.parse(Sources.snapshot || "{}").panels || []).filter(panel => String(Sources.display(panel.id, Sources.revision)) !== "") }

    function mount(item) { loadFailed = false; portal = item }
    function unmount(item) { if (portal === item) { requested = false; surface = null; portal = null; restoreWindow() } }
    function bind(item) {
        loadFailed = false
        surface = item
        if (!provider) { requested = false; return }
        provider.host = mode
        requested = true
        Qt.callLater(mode.focusSurface)
        if (pendingPlugin === pluginId) {
            const action = pendingAction
            pendingPlugin = ""; pendingAction = ""
            if (action && action !== "enter") provider.command(action)
        }
    }
    function focusSurface() { if (surface && surface.forceActiveFocus) surface.forceActiveFocus() }
    function ask(id, action) {
        if (!String(Sources.display(id, Sources.revision))) return
        if (id === pluginId && provider) {
            if (action === "enter") requested = true
            else if (action === "leave") leave()
            else provider.command(action)
        } else {
            pendingPlugin = id; pendingAction = action
            Navigation.goKey("source", id)
        }
    }
    function leave() {
        const id = pluginId
        requested = false
        restoreWindow()
        if (Navigation.screen === "source" && Navigation.routeKey === id) {
            Navigation.back()
            if (Navigation.screen === "source" && Navigation.routeKey === id) Navigation.go("home", 0)
        }
        if (window) window.contentItem.forceActiveFocus()
    }
    function restoreWindow() {
        if (window && previousGeometry) {
            window.visibility = previousGeometry.visibility
            if (previousGeometry.visibility === Window.Windowed) { window.width = previousGeometry.width; window.height = previousGeometry.height; window.x = previousGeometry.x; window.y = previousGeometry.y }
        } else if (changedFullscreen && window && window.visibility === Window.FullScreen) window.visibility = Window.Windowed
        changedFullscreen = false
        previousGeometry = null
    }
    onRequestedChanged: {
        if (requested && window && previousGeometry === null) previousGeometry = {width:window.width,height:window.height,x:window.x,y:window.y,visibility:window.visibility}
        else if (!requested) restoreWindow()
    }
    function toggleFullscreen() {
        if (!window) return
        if (window.visibility === Window.FullScreen) { window.visibility = Window.Windowed; changedFullscreen = false }
        else { changedFullscreen = true; window.showFullScreen() }
    }
    function menuFor(id) {
        const open = active && id === pluginId
        const entries = [{text:open ? I18n.tr("Leave view", I18n.rev) : I18n.tr("Open view", I18n.rev), act:"display", plugin:id, command:open ? "leave" : "enter"}]
        if (open && provider) {
            entries.push({text:I18n.tr("Fullscreen", I18n.rev), act:"display", plugin:id, command:"fullscreen", checked:window && window.visibility === Window.FullScreen})
            for (const item of provider.menuEntries || []) entries.push(Object.assign({}, item, {act:"display",plugin:id}))
        } else entries.push({text:I18n.tr("Settings", I18n.rev), act:"display", plugin:id, command:"settings"})
        return entries
    }
    readonly property var approvalWatch: Connections {
        target: Sources
        function onRevisionChanged() { if (mode.portal && String(mode.entry) === "") { mode.requested = false; mode.surface = null; mode.restoreWindow() } }
    }
}
