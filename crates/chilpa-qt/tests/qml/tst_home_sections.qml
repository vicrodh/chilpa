import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Screens"

// The Home's shelves come in the order chosen, without the ones turned off,
// with the sources' among them; Customize opens over them.
TestCase {
    name: "HomeSections"
    when: windowShown
    visible: true
    width: 1200
    height: 800

    HomeScreen { id: home; width: 1200; height: 800 }
    function init() { tryCompare(home, "presented", true); wait(40) }

    function shelves() {
        return Array.from({length: home.model.count}, (_, index) => home.model.get(index).key)
    }
    function cleanup() {
        const customizer = findChild(home, "homeCustomizer")
        if (customizer && customizer.opened) customizer.close()
        Sources.show({homeLibraries: []})
        home.keep([])
        wait(40)
    }

    function test_the_library_shelves_come_first_in_their_own_order() {
        compare(shelves(), ["library.recentlyPlayed", "everywhere.recentlyPlayed", "library.recentPlaylists", "everywhere.recentPlaylists", "library.recentlyAdded", "library.mostPlayed"])
    }

    function test_a_shelf_turned_off_is_gone_and_a_moved_one_moves() {
        home.change(5, "enabled", false)
        tryVerify(() => shelves().length === 5)
        compare(shelves(), ["library.recentlyPlayed", "everywhere.recentlyPlayed", "library.recentPlaylists", "everywhere.recentPlaylists", "library.recentlyAdded"])
        home.move(1, -1)
        tryVerify(() => shelves()[0] === "everywhere.recentlyPlayed")
        compare(shelves(), ["everywhere.recentlyPlayed", "library.recentlyPlayed", "library.recentPlaylists", "everywhere.recentPlaylists", "library.recentlyAdded"])
        const kept = JSON.parse(SettingsModel.kept("ui.home.sections"))
        compare(kept[0].id, "everywhere.recentlyPlayed", "kept for the next time")
        compare(kept[5].enabled, false)
        home.keep([])
        tryVerify(() => shelves().length === 6, 1000, "restoring brings them back")
    }

    function test_a_source_s_shelf_joins_after_the_library_s() {
        Sources.pages = {
            "fixture native:discover:home": JSON.stringify({shelves: [
                {id: "recent", title: "Recently played", key: "fixture native:discover:home|recent", items: [{ref: "album:1"}], enabled: true},
                {id: "watch", title: "Release watch", key: "fixture native:discover:home|watch", items: [{ref: "album:2"}], enabled: false}]})
        }
        Sources.show({homeLibraries: [{id: "fixture", name: "Fixture", enabled: true}]})
        home.homeRefresh++
        tryVerify(() => shelves().length === 7)
        compare(shelves()[6], "source:fixture:recent")
        compare(home.sections.length, 8, "disabled source shelves remain available for Customize")
        home.change(7, "enabled", true)
        tryVerify(() => shelves().length === 8, 1000, "a disabled source shelf can be chosen for Home")
        home.change(7, "enabled", false)
        home.move(6, -6)
        tryVerify(() => shelves()[0] === "source:fixture:recent", 1000, "it goes wherever it is put")
        Sources.show({homeLibraries: []})
    }

    function test_a_late_source_does_not_remount_local_shelves() {
        const first = findChild(home.contentItem, "homeShelf")
        verify(first !== null)
        Sources.pages = { "fixture native:discover:home": JSON.stringify({ shelves: [
            { id: "late", title: "Late source", items: [{ ref: "album:1" }], enabled: true }
        ] }) }
        Sources.show({ homeLibraries: [{ id: "fixture", name: "Fixture", enabled: true }] })
        home.homeRefresh++
        tryVerify(() => shelves().includes("source:fixture:late"))
        compare(findChild(home.contentItem, "homeShelf"), first, "the mounted shelf keeps its delegate")
    }
    function test_consolidated_source_module_is_one_choice_and_retains_home_settings() {
        home.keep([{id:"source:fixture:interest:artist",enabled:false,size:12},
            {id:"source:fixture:interest:album",enabled:false,size:24}])
        Sources.pages = {"fixture native:discover:home": JSON.stringify({shelves:[
            {id:"interest:mixed",title:"Recommendations",kind:"mixed",key:"fixture native:discover:home|interest:mixed",
                previous_ids:["interest:artist","interest:album"],items:[{ref:"artist:1"},{ref:"album:2"}]}]})}
        Sources.show({homeLibraries:[{id:"fixture",name:"Fixture",enabled:true}]})
        home.homeRefresh++
        tryVerify(() => home.sections.some(section => section.id === "source:fixture:interest:mixed"))
        const module = home.sections.filter(section => section.sourceId === "fixture")
        compare(module.length, 1)
        compare(module[0].enabled, false)
        compare(module[0].size, 12)
        verify(!shelves().includes(module[0].id), "a disabled consolidated module stays hidden")
    }

    // A shelf with records shows; one without stays out of the way.
    function test_a_shelf_with_records_shows_and_an_empty_one_does_not() {
        const loaders = []
        const walk = item => { for (const child of item.children) { if (child.objectName === "homeShelf") loaders.push(child); walk(child) } }
        walk(home.contentItem)
        const added = loaders.find(l => l.entry.id === "library.recentlyAdded")
        tryVerify(() => added.visible && added.height > 0, 1000, "the records added show")
        tryVerify(() => findChild(added, "albumCarouselRail") !== null)
        compare(findChild(added, "albumCarouselRail").motionViewport, home)
        tryVerify(() => loaders.find(l => l.entry.id === "library.recentlyPlayed").height === 0, 1000, "nothing played, nothing shown")
    }

    function shelfOf(id) {
        const found = []
        const walk = item => { for (const child of item.children) { if (child.objectName === "homeShelf") found.push(child); walk(child) } }
        walk(home.contentItem)
        return found.find(shelf => shelf.entry.id === id)
    }
    function slimRowsOf(item) {
        const found = []
        const walk = node => { if (node.hasOwnProperty("subtitle") && node.hasOwnProperty("count") && node.visible) found.push(node); for (const child of node.children || []) walk(child) }
        walk(item)
        return found
    }

    // The grid's own count of placed rows once shared its name with the
    // module's "has anything", and Most played drew its first track only.
    function test_most_played_as_a_grid_shows_every_track_it_places() {
        const most = shelfOf("library.mostPlayed")
        tryVerify(() => most.height > 0, 1000, "the tracks played show")
        tryVerify(() => slimRowsOf(most).length === 24, 1000, "two pages of twelve")
        compare(slimRowsOf(most)[1].title, "Track 1")
    }

    function test_customize_lists_every_shelf() {
        findChild(home, "homeCustomize").clicked()
        const list = findChild(home, "customizeSourceChoices")
        tryVerify(() => list !== null && list.count === 6)
        compare(findChild(home, "homeCustomizer").groups[0].name, "Chilpa")
    }

    function test_many_plugin_modules_only_mount_the_viewport() {
        const modules = Array.from({length: 40}, (_, index) => ({id:"module-" + index, title:"Module " + index,
            key:"fixture native:discover:home|module-" + index, items:[{ref:"album:1"}], enabled:true}))
        Sources.pages = {"fixture native:discover:home": JSON.stringify({shelves:modules})}
        Sources.show({homeLibraries:[{id:"fixture", name:"Fixture", enabled:true}]})
        home.homeRefresh++
        tryVerify(() => shelves().length === 46)
        wait(100)
        let mounted = 0
        const walk = item => { for (const child of item.children) { if (child.objectName === "homeModuleLoader" && child.item) mounted++; walk(child) } }
        walk(home.contentItem)
        verify(mounted < 16, "forty modules must not construct forty card rails")
        tryVerify(() => findChild(home.contentItem, "sourceShelfRail") !== null)
        compare(findChild(home.contentItem, "sourceShelfRail").motionViewport, home)
    }

    function test_switching_customizer_source_keeps_the_modal_open() {
        Sources.show({homeLibraries: [{id: "fixture", name: "Fixture", enabled: true}]})
        home.homeRefresh++
        findChild(home, "homeCustomize").clicked()
        const customizer = findChild(home, "homeCustomizer")
        tryVerify(() => customizer.groups.length > 1)
        customizer.open()
        tryVerify(() => customizer.opened)
        const tab = findChild(customizer.contentItem, "customizeGroup_fixture")
        verify(tab !== null)
        tryVerify(() => tab.x > 0, 1000, "the tabs have finished positioning")
        mouseClick(tab, tab.width / 2, tab.height / 2)
        compare(customizer.activeGroup, "fixture")
        verify(customizer.opened, "changing tabs must not dismiss the dialog")
        customizer.close()
        Sources.show({homeLibraries: []})
    }
}
