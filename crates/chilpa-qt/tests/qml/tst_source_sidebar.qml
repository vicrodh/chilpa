import QtQuick
import QtTest
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../../qml/Patterns"

TestCase {
    name: "SourceSidebar"
    when: windowShown
    width: 960
    height: 600
    visible: true
    Sidebar { id: sidebar; width: 240; height: 600; collapsed: false }

    function init() {
        Navigation.screen = "source"
        Navigation.routeKey = "radio"
        sidebar.collapsed = false
        sidebar.expandedSources = {radio: true}
        Sources.asked = []
        Sources.show({ navigation_id: "collection", panels: [{ id: "radio", name: "Source", icon: "letterS",
            children: [{ id: "collection", title: "Collection" }] }] })
        wait(30)
    }

    function test_child_uses_the_plugins_opaque_identity() {
        const child = findChild(sidebar, "sourceChild")
        verify(child.visible)
        verify(child.active)
        mouseClick(child)
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
