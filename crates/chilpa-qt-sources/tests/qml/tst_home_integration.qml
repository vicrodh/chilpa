import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"

TestCase {
    name: "HomeIntegration"
    when: windowShown
    visible: true
    width: 1000; height: 800
    SourceHomeSections { id: home; width: parent.width }
    Component { id: temporary; SourceHomeSections { rendered: false } }
    function test_a_pending_refresh_keeps_cached_module_descriptors() {
        Sources.pages = {"cached native:discover:home": JSON.stringify({shelves:[
            {id:"record",title:"Records",items:[{ref:"album:1"}]}
        ]})}
        Sources.show({homeLibraries:[{id:"cached",name:"Fixture",enabled:true}]})
        tryCompare(home, "ready", true)
        tryVerify(() => home.sections.length === 1 && home.sections[0].id === "record")
        Sources.pages = {"cached native:discover:home": JSON.stringify({loading:true,shelves:[]})}
        home.refresh++
        wait(100)
        compare(home.sections.length, 1)
        compare(home.sections[0].id, "record")
    }
    function test_pending_refresh_is_cancelled_when_the_page_is_destroyed() {
        const page = createTemporaryObject(temporary, this)
        verify(page)
        page.refresh++
        page.destroy()
        wait(50)
    }
    function test_selected_source_sections_join_home() {
        Sources.pages = {
            "fixture native:discover:home": JSON.stringify({shelves:[
                {id:"recent",title:"Recently played",key:"fixture native:discover:home|recent",items:[{ref:"album:1"}],enabled:true},
                {id:"hidden",title:"Hidden",items:[{ref:"album:2"}],enabled:false},
                {id:"empty",title:"Empty",items:[],enabled:true}
            ]})
        }
        Sources.show({homeLibraries:[{id:"fixture",name:"Fixture",enabled:true}]})
        tryVerify(() => home.sections.length === 3)
        compare(home.sections.filter(s => s.enabled && s.items.length).length, 1)
        compare(home.sections[0].title, "Fixture · Recently played")
        SettingsModel.keep("ui.source.fixture.discover.home", JSON.stringify([{id:"recent",enabled:false}]))
        home.refresh++
        tryVerify(() => home.sections.filter(s => s.enabled && s.items.length).length === 0)
    }
}
