import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/SectionChoices.js" as SectionChoices

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
    function test_consolidated_module_retains_order_size_and_visibility_once() {
        const available = [{id:"before"}, {id:"mixed", previous_ids:["old:artist", "old:album", "old:playlist"]}, {id:"after"}]
        const previous = [{id:"after"}, {id:"old:artist", enabled:false, size:12}, {id:"old:album", enabled:true, size:24},
            {id:"old:playlist", enabled:false}, {id:"old:album", enabled:true}, {id:"missing"}]
        const rows = SectionChoices.order(available, previous)
        compare(rows.map(row => row.id), ["after", "mixed", "before"])
        compare(rows[1].enabled, true)
        compare(rows[1].size, 12)
        const exact = SectionChoices.order(available, previous.concat([{id:"mixed", enabled:false, size:6}]))
        compare(exact[1].enabled, false)
        compare(exact[1].size, 6)
        compare(SectionChoices.order(available, [{id:"old:album",enabled:false}])[0].enabled, false)
    }
}
