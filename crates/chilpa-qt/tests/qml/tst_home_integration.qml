import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"

TestCase {
    name: "HomeIntegration"
    when: windowShown
    visible: true
    width: 1000; height: 800
    HomeScreen { id: home; anchors.fill: parent }
    function test_selected_source_sections_join_home() {
        Sources.pages = {
            "fixture native:discover:home": JSON.stringify({shelves:[
                {id:"recent",title:"Recently played",key:"fixture native:discover:home|recent",items:[{ref:"album:1"}],enabled:true},
                {id:"hidden",title:"Hidden",items:[{ref:"album:2"}],enabled:false},
                {id:"empty",title:"Empty",items:[],enabled:true}
            ]})
        }
        Sources.show({homeLibraries:[{id:"fixture",name:"Fixture"}]})
        tryVerify(() => home.homeSections.length === 1)
        compare(home.homeSections[0].title, "Fixture · Recently played")
        SettingsModel.keep("ui.source.fixture.discover.home", JSON.stringify([{id:"recent",enabled:false}]))
        home.homeRefresh++
        tryVerify(() => home.homeSections.length === 0)
    }
}
