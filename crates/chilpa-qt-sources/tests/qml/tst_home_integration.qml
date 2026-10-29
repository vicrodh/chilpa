import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"
import "../../../chilpa-qt/qml/Screens" as Core

TestCase {
    id: test
    name: "HomeIntegration"
    when: windowShown
    visible: true
    width: 1000; height: 800
    SourceHomeSections { id: home; width: parent.width }
    Component { id: temporary; SourceHomeSections { rendered: false } }
    Component { id: screenFactory; Core.HomeScreen { width: test.width; height: test.height; presented: true } }
    SignalSpy { id: reads; target: Sources; signalName: "pageRequested" }

    function test_customize_reads_an_opted_out_source_only_when_inspected_and_remembers_a_module() {
        SettingsModel.keep("ui.home.sections", "[]")
        Sources.pages = {"opted-out native:discover:home": JSON.stringify({shelves:[
            {id:"choice",title:"Choose this module",kind:"album",items:[{ref:"album:1"}]}
        ]})}
        Sources.show({homeLibraries:[{id:"opted-out",name:"Optional source",enabled:false}]})
        const page = createTemporaryObject(screenFactory, test)
        verify(page)
        wait(100)
        reads.clear()
        Library.remoteRev++
        wait(100)
        compare(reads.count, 0, "an unselected source does not start background reads")
        const button = findChild(page, "homeCustomize")
        verify(button)
        mouseClick(button, button.width / 2, button.height / 2)
        const dialog = findChild(page, "homeCustomizer")
        tryVerify(() => dialog && dialog.opened)
        const tab = findChild(dialog.contentItem, "customizeGroup_opted-out")
        verify(tab)
        waitForRendering(tab)
        mouseClick(tab, tab.width / 2, tab.height / 2)
        tryCompare(dialog, "activeGroup", "opted-out")
        const sourceModules = findChild(page, "homeSourceSections")
        verify(sourceModules.active)
        compare(sourceModules.browsingSource, "opted-out")
        tryVerify(() => page.homeSections.some(section => section.sourceId === "opted-out"))
        verify(reads.count > 0)
        tryVerify(() => findChild(dialog.contentItem, "sourceModuleToggle_source:opted-out:choice") !== null)
        const toggle = findChild(dialog.contentItem, "sourceModuleToggle_source:opted-out:choice")
        verify(!toggle.checked, "browsing alone must not enable a module")
        mouseClick(toggle, toggle.width / 2, toggle.height / 2)
        tryVerify(() => JSON.parse(SettingsModel.kept("ui.home.sections")).some(section => section.id === "source:opted-out:choice" && section.enabled))
        dialog.close()
        page.visible = false
        const reopened = createTemporaryObject(screenFactory, test)
        verify(reopened)
        tryVerify(() => reopened.sections.some(section => section.id === "source:opted-out:choice" && section.enabled))
        reads.clear()
        Library.remoteRev++
        tryVerify(() => reads.count > 0, 1000, "the explicit choice keeps loading after Customize closes")
        SettingsModel.keep("ui.home.sections", "[]")
    }
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
