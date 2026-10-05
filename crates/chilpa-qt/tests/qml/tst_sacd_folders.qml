import QtQuick
import QtTest
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../../qml/Screens"
import "../../qml/Patterns"

TestCase {
    name: "OpenMediaNavigation"
    when: windowShown
    visible: true
    width: 1280
    height: 720
    OpenMediaScreen { id: screen; anchors.fill: parent }
    NavMenu { id: navigation }

    function test_three_open_actions_have_their_own_view() {
        for (const pair of [["openFolder", "folder"], ["openCd", "cd"], ["openSacd", "sacd"]]) {
            const button = findChild(screen, pair[0])
            verify(button !== null)
            mouseClick(button)
            compare(OpenMedia.picked, pair[1])
        }
    }
    function test_closing_discards_the_loaded_media() {
        OpenMedia.snapshot = JSON.stringify({title: "Temporary album", count: 2})
        const button = findChild(screen, "closeMedia")
        tryCompare(button, "visible", true)
        mouseClick(button)
        compare(OpenMedia.snapshot, "{}")
        compare(OpenMedia.closes, 1)
        compare(Navigation.page, "openMedia")
    }
    function test_menu_includes_open_media_and_tracks_plugin_changes() {
        const menu = findChild(navigation, "navigationMenu")
        let index = navigation.places.findIndex(p => p.screen === "openMedia")
        verify(index >= 0)
        menu.chosen(index)
        compare(Navigation.page, "openMedia")
        Sources.show({panels: [{id: "example", name: "Example", icon: "disc", music: true}]})
        index = navigation.places.findIndex(p => p.key === "example")
        verify(index >= 0)
        menu.chosen(index)
        compare(Navigation.went, ["source", "example"])
        Sources.show({panels: []})
        compare(navigation.places.findIndex(p => p.key === "example"), -1)
    }
}
