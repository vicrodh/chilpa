import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Screens"

TestCase {
    name: "SourceScreen"
    when: windowShown
    visible: true
    width: 900
    height: 500

    SourceScreen { id: screen; width: 640; height: 500 }
    SourceEntryScreen { id: entryPage; x: 650; width: 250; height: 500 }

    readonly property var station: ({ id: "station:1", key: "radio station:1", title: "One FM", subtitle: "Somewhere", image: "", kind: "item", live: true, finite: false, meta: "", progress: 0, played: false, heard: "", playable: true, whyNot: "", saved: false, sounding: false, resolving: false })
    readonly property var folder: ({ id: "countries", key: "radio countries", title: "By country", subtitle: "", image: "", kind: "folder", live: false, finite: false, meta: "", progress: 0, played: false, heard: "", playable: true, whyNot: "", saved: false, sounding: false, resolving: false })
    readonly property var episode: Object.assign({}, station, { id: "ep:1", key: "shows ep:1", title: "An Episode", live: false, finite: true, meta: "2026-09-25 · 22:05", progress: 0.4, heard: "40 %" })

    function init() {
        Sources.asked = []
        Sources.show({ crumbs: [], entries: [folder, station, episode], error: "", loading: false, more: false, placeSavable: false, detail: null })
        wait(30)
    }

    function last() { return Sources.asked[Sources.asked.length - 1] }
    function row(index) { return findChild(screen, "sourceList").itemAtIndex(index) }

    function test_opening_asks_for_the_source_of_the_route() {
        Sources.asked = []
        const fresh = createTemporaryObject(Qt.createComponent("../../qml/Screens/SourceScreen.qml"), this)
        compare(Sources.asked[0], ["open", "radio", ""])
    }

    function test_a_folder_is_entered_and_a_station_played_by_the_same_click() {
        mouseClick(row(0), 60, 10)
        compare(last(), ["enter", "countries", ""])
        mouseClick(row(1), 60, 10)
        compare(last(), ["enter", "station:1", ""])
    }

    function test_the_heart_saves_and_does_not_play() {
        mouseClick(row(1), row(1).width - 22, row(1).height / 2)
        compare(last(), ["save", "station:1", ""])
    }

    function test_a_crumb_goes_back_to_its_step() {
        Sources.show({ crumbs: ["By country", "Mexico"] })
        wait(30)
        const crumbs = Array.from(findChild(screen, "sourceCrumbs").children).filter(c => c.text !== undefined)
        compare(crumbs.length, 3)
        mouseClick(crumbs[1])
        compare(last(), ["crumb", "1", ""])
        mouseClick(crumbs[0])
        compare(last(), ["crumb", "0", ""])
    }

    function test_an_error_is_said_and_can_be_retried() {
        Sources.show({ entries: [], error: "the directory did not answer" })
        wait(30)
        compare(findChild(screen, "sourceStatus").text, "the directory did not answer")
    }

    function test_an_episode_is_queued_by_its_button_and_shows_where_it_was_left() {
        const row = this.row(2)
        compare(row.detail, "2026-09-25 · 22:05")
        compare(row.duration, "40 %")
        mouseClick(row, row.width - 22, row.height / 2)
        compare(last(), ["queue", "ep:1", ""])
    }

    function test_the_right_button_opens_the_entry_s_own_page() {
        mouseClick(row(1), 60, 10, Qt.RightButton)
        compare(Navigation.went, ["sourceEntry", "radio station:1"])
    }

    function test_a_show_is_shown_above_its_episodes_and_they_can_be_put_in_another_order() {
        Sources.show({ crumbs: ["A Show"], entries: [episode], sortable: true, order: "", about: { title: "A Show", subtitle: "Somebody", image: "", description: "What it is about.", facts: [] } })
        wait(30)
        compare(findChild(screen, "aboutDescription").text, "What it is about.")
        Sources.show({ about: null })
        wait(30)
        compare(findChild(screen, "sourceList").headerItem.height, 0, "a place with nothing to say takes no room")
    }

    function test_a_place_that_can_be_saved_shows_its_heart() {
        Sources.show({ crumbs: ["A Show"], placeSavable: true, placeSaved: false })
        wait(30)
        mouseClick(screen, 640 - 28 - 340 - 8 - 14, 14 + 20)
        compare(last(), ["save", "", ""])
    }

    function test_the_page_of_an_entry_says_what_is_on_and_saves_it() {
        compare(Sources.asked.some(a => a[0] === "detail"), false, "a key that names no entry asks for none")
        Navigation.screen = "sourceEntry"
        Navigation.routeKey = "radio station:1"
        wait(30)
        compare(Sources.asked.some(a => a[0] === "open" && a[1] === "radio station:1"), false, "the list does not take another screen's key for a plugin")
        verify(Sources.asked.some(a => a[0] === "detail" && a[1] === "station:1" && a[2] === "radio"))
        Navigation.screen = "source"
        Navigation.routeKey = "radio"
        Sources.show({ detail: Object.assign({}, station, { source: "radio", name: "Radio", description: "", facts: [{ label: "Country", value: "Somewhere" }], song: "Someone - A Song", art: "" }) })
        wait(30)
        compare(findChild(entryPage, "entryHero").name, "One FM")
        verify(findChild(entryPage, "entrySong").text.includes("Someone - A Song"))
        mouseClick(findChild(entryPage, "entrySave"))
        compare(last(), ["save", "station:1", "radio"])
    }
}
