import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "DiscoveryControls"
    when: windowShown
    visible: true
    width: 1000; height: 800
    SourceDiscoveryControls { id: controls; anchors.right: parent.right; about: ({plugin:"fixture",tab:"home",genres:[],shelves:[]}) }
    function init() { controls.about = {plugin:"fixture",tab:"home",genres:[],shelves:[]} }
    function test_missing_genres_hide_the_button_but_keep_customization() {
        verify(!findChild(controls, "discoveryGenres").visible)
        verify(findChild(controls, "discoveryCustomize").visible)
    }
    function test_genres_open_as_a_flyout_and_escape_closes_it() {
        controls.about = {plugin:"fixture",tab:"home",genres:[{id:"1",name:"Rock"}],shelves:[]}
        const button = findChild(controls, "discoveryGenres")
        const popup = findChild(controls, "discoveryGenrePopup")
        verify(button.visible)
        waitForRendering(controls)
        mouseClick(button)
        tryCompare(popup, "opened", true)
        verify(popup.y >= controls.height)
        keyClick(Qt.Key_Escape)
        tryCompare(popup, "opened", false)
    }
}
