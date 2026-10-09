import QtQuick
import QtTest
import app.chilpa.sources
import "../../qml/Patterns"

TestCase {
    name: "AlbumLibraries"
    when: windowShown
    visible: true
    width: 400
    height: 250
    SourceAlbumLibraries { id: controls; active: true; width: 360 }
    function init() {
        controls.tracks = false
        Sources.asked = []
        Sources.show({albumLibraries:[{id:"fixture.one",name:"First library",enabled:false,icon:""},{id:"fixture.two",name:"Second library",enabled:true,icon:""}]})
        wait(20)
    }
    function test_independent_source_toggles_follow_confirmed_state() {
        let first = findChild(controls, "albumLibraryToggle-fixture.one")
        verify(first)
        compare(first.checked, false)
        mouseClick(first)
        compare(Sources.asked[0], ["album_library", "fixture.one", "true"])
        compare(first.checked, false)
        Sources.show({albumLibraries:[{id:"fixture.one",name:"First library",enabled:true,icon:""},{id:"fixture.two",name:"Second library",enabled:true,icon:""}]})
        wait(20)
        first = findChild(controls, "albumLibraryToggle-fixture.one")
        compare(first.checked, true)
        const second = findChild(controls, "albumLibraryToggle-fixture.two")
        mouseClick(second)
        compare(Sources.asked[1], ["album_library", "fixture.two", "false"])
        compare(first.checked, true)
    }
    function test_tracks_toggle_does_not_change_album_integration() {
        controls.tracks = true
        Sources.show({trackLibraries:[{id:"fixture.one",name:"First library",enabled:false,icon:""}]})
        wait(20)
        const toggle = findChild(controls,"albumLibraryToggle-fixture.one")
        verify(toggle)
        mouseClick(toggle)
        compare(Sources.asked[0],["track_library","fixture.one","true"])
        compare(toggle.checked,false)
    }
    function test_no_controls_without_an_available_album_library() {
        Sources.show({albumLibraries:[]})
        tryCompare(controls, "visible", false)
    }
}
