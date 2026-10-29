import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.kit
import "../../../chilpa-qt/qml/Patterns"

TestCase {
    name: "SourceSidebar"
    when: windowShown
    width: 960
    height: 600
    visible: true
    QtObject {
        id: navigationRouterFixture
        property string phase: "ready"
        property int generation: 0
        property string route: "source:radio"
        signal preparingRoute()
        function request(force) { generation++ }
    }
    Sidebar { id: sidebar; width: 240; height: 600; collapsed: false; router: navigationRouterFixture }

    function init() {
        SettingsModel.preferences = ({})
        Prefs.rev++
        Navigation.screen = "source"
        Navigation.routeKey = "radio"
        sidebar.collapsed = false
        sidebar.expandedSources = {radio: true}
        Sources.asked = []
        Sources.show({ navigation_id: "collection", panels: [{ id: "radio", name: "Source", icon: "letterS",
            children: [{ id: "collection", title: "Collection" }] }] })
        wait(30)
    }

    function test_device_shortcut_can_be_hidden_and_restored() {
        const entry = findChild(sidebar, "deviceNavigation")
        verify(entry)
        compare(entry.visible, true)
        SettingsModel.preferences = {"navigation.devices":"false"}
        Prefs.rev++
        // Hiding the shortcut drops it from the navigation model, so the
        // delegate is destroyed; showing it again builds a new one.
        tryCompare(sidebar, "devicesVisible", false)
        compare(findChild(sidebar, "deviceNavigation"), null)
        SettingsModel.preferences = {"navigation.devices":"true"}
        Prefs.rev++
        tryVerify(() => findChild(sidebar, "deviceNavigation") !== null)
        compare(findChild(sidebar, "deviceNavigation").visible, true)
    }

    function test_global_playlist_grouping_can_collapse_and_return_to_one_list() {
        const list = {id: -2, name: "Cloud", track_count: 8, source: "radio", provider: "Source", mixed: false, artwork_path: "", folder_id: null, visible_sidebar: true, covers: [], source_icon: "", separate: false}
        Playlists.catalog = {folders: [], playlists: [list]}
        Playlists.rev++
        wait(20)
        const section = findChild(sidebar, "sidebarPlaylists")
        compare(section.rows.length, 1)
        verify(section.rows[0].showSourceBadge)
        Playlists.catalog = {group_sources:true,folders: [], playlists: [list]}
        Playlists.rev++
        wait(20)
        compare(section.rows[0].rowKind, 2)
        compare(section.rows.length, 2)
        section.toggleGroup(section.rows[0].expandKey)
        compare(section.rows.length, 1)
        Playlists.catalog = {group_sources:false,folders:[],playlists:[list]}
        Playlists.rev++
        wait(20)
        compare(section.rows.length,1)
        compare(section.rows[0].rowKind,0)
        verify(section.rows[0].showSourceBadge)
        Playlists.catalog = {folders: [], playlists: []}
        Playlists.rev++
    }
    function test_child_uses_the_plugins_opaque_identity() {
        const child = findChild(sidebar, "sourceChild")
        verify(child.visible)
        verify(child.active)
        mouseClick(child)
        compare(Sources.asked.length, 0, "source work waits for the navigation acknowledgement")
        navigationRouterFixture.preparingRoute()
        compare(Sources.asked[Sources.asked.length - 1], ["child", "radio", "collection"])
        compare(Navigation.went, ["source", "radio"])
    }

    function test_children_collapse_only_through_the_source_caret() {
        const child = findChild(sidebar, "sourceChild")
        sidebar.collapsed = true
        verify(!child.visible)
        sidebar.collapsed = false
        Navigation.routeKey = "another"
        verify(child.visible)
        mouseClick(findChild(findChild(sidebar, "sourceParent"), "sourceCaret"))
        verify(!child.visible)
        compare(Sources.asked.length, 0)
    }
}
