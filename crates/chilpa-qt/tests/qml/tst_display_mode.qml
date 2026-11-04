import QtQuick
import QtQuick.Window
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Components" as Presentation

TestCase {
    id: test
    name: "DisplayMode"
    when: windowShown
    visible: true; width: 900; height: 700
    Item {
        id: owner
        width: 900; height: 700
        property int visibility: Window.Windowed
        property var contentItem: owner
        function showFullScreen() { visibility = Window.FullScreen }
    }
    Item { id: portal; property string pluginId: "fixture" }
    Item {
        id: surface
        property var display: QtObject {
            property int version: 1
            property var host: null
            property bool useCustomPlayer: false
            property Component playerComponent: null
            property real playerExtent: 112
            property string playerDock: "bottom"
            property var trayTarget: null
            property var menuEntries: [{text:"Settings",command:"settings"}]
            property var actions: []
            function command(action) { actions = actions.concat([action]) }
        }
    }
    Component { id: futureDeck; Item {} }
    readonly property var mode: Presentation.DisplayMode
    function init() {
        mode.requested = false; mode.surface = null; mode.portal = null
        owner.visibility = Window.Windowed; owner.width = 900; owner.height = 700
        mode.window = owner; mode.pendingPlugin = ""; mode.pendingAction = ""
        Sources.displays = {fixture:"file:///fixture.qml"}
        Sources.show({panels:[{id:"fixture",name:"A display"}]})
        surface.display.version = 1; surface.display.actions = []
        surface.display.useCustomPlayer = false; surface.display.playerComponent = null
    }
    function cleanup() { mode.requested=false;mode.surface=null;mode.portal=null;mode.window=null;Sources.displays={};Sources.show({panels:[]}) }
    function test_approved_display_enters_and_restores_the_window() {
        mode.mount(portal); mode.bind(surface)
        verify(mode.active); compare(surface.display.host,mode)
        mode.toggleFullscreen(); compare(owner.visibility,Window.FullScreen)
        mode.leave(); verify(!mode.active); compare(owner.visibility,Window.Windowed)
        compare(owner.width,900); compare(owner.height,700)
    }
    function test_future_player_is_opt_in_and_needs_a_component() {
        mode.mount(portal);mode.bind(surface)
        surface.display.useCustomPlayer=true
        verify(!mode.customPlayer)
        surface.display.playerComponent=futureDeck
        verify(mode.customPlayer)
        surface.display.useCustomPlayer=false
        verify(!mode.customPlayer)
        compare(mode.surface,surface)
    }
    function test_revoked_permission_removes_the_mode_and_restores_fullscreen() {
        mode.mount(portal);mode.bind(surface);mode.toggleFullscreen()
        Sources.displays={};Sources.revision++
        tryVerify(() => !mode.active)
        compare(mode.surface,null);compare(owner.visibility,Window.Windowed)
        mode.ask("fixture","enter")
        verify(!mode.active)
    }
    function test_submenu_can_open_settings_before_the_view_is_loaded() {
        mode.ask("fixture","settings")
        compare(Navigation.went[0],"source");compare(Navigation.went[1],"fixture")
        mode.mount(portal);mode.bind(surface)
        compare(surface.display.actions[0],"settings")
        verify(mode.menuFor("fixture").some(entry => entry.command === "settings"))
    }
    function test_unknown_contract_stays_embedded() {
        surface.display.version=99
        mode.mount(portal);mode.bind(surface)
        verify(!mode.active);compare(mode.provider,null)
    }
    function test_old_portal_teardown_does_not_remove_its_replacement() {
        const other=createTemporaryQmlObject('import QtQuick; Item { property string pluginId: "fixture" }',test)
        mode.mount(portal);mode.mount(other);mode.unmount(portal)
        compare(mode.portal,other)
        mode.unmount(other);compare(mode.portal,null)
    }
}
