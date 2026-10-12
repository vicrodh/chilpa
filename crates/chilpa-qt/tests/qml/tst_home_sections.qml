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

    function shelves() {
        const found = []
        const walk = item => { for (const child of item.children) { if (child.objectName === "homeShelf") found.push(child.modelData.id); walk(child) } }
        walk(home.contentItem)
        return found
    }
    function cleanup() { home.keep([]) }

    function test_the_library_shelves_come_first_in_their_own_order() {
        compare(shelves(), ["library.recentlyPlayed", "everywhere.recentlyPlayed", "library.recentlyAdded", "library.mostPlayed"])
    }

    function test_a_shelf_turned_off_is_gone_and_a_moved_one_moves() {
        home.change(3, "enabled", false)
        compare(shelves(), ["library.recentlyPlayed", "everywhere.recentlyPlayed", "library.recentlyAdded"])
        home.move(1, -1)
        compare(shelves(), ["everywhere.recentlyPlayed", "library.recentlyPlayed", "library.recentlyAdded"])
        const kept = JSON.parse(SettingsModel.kept("ui.home.sections"))
        compare(kept[0].id, "everywhere.recentlyPlayed", "kept for the next time")
        compare(kept[3].enabled, false)
        home.keep([])
        compare(shelves().length, 4, "restoring brings them back")
    }

    function test_a_source_s_shelf_joins_after_the_library_s() {
        Sources.pages = {
            "fixture native:discover:home": JSON.stringify({shelves: [{id: "recent", title: "Recently played", key: "fixture native:discover:home|recent", items: [{ref: "album:1"}], enabled: true}]})
        }
        Sources.show({homeLibraries: [{id: "fixture", name: "Fixture"}]})
        home.homeRefresh++
        tryVerify(() => shelves().length === 5)
        compare(shelves()[4], "source:fixture:recent")
        home.move(4, -4)
        compare(shelves()[0], "source:fixture:recent", "it goes wherever it is put")
        Sources.show({homeLibraries: []})
    }

    // A shelf with records shows; one without stays out of the way.
    function test_a_shelf_with_records_shows_and_an_empty_one_does_not() {
        const loaders = []
        const walk = item => { for (const child of item.children) { if (child.objectName === "homeShelf") loaders.push(child); walk(child) } }
        walk(home.contentItem)
        const added = loaders.find(l => l.modelData.id === "library.recentlyAdded")
        tryVerify(() => added.visible && added.height > 0, 1000, "the records added show")
        verify(!loaders.find(l => l.modelData.id === "library.recentlyPlayed").visible, "nothing played, nothing shown")
    }

    function test_customize_lists_every_shelf() {
        findChild(home, "homeCustomize").clicked()
        const list = findChild(home, "customizeChoices")
        tryVerify(() => list !== null && list.count === home.sections.length)
    }
}
