import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import app.chilpa.playlists
import "../../qml/Components"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    id: test
    name: "SearchSelection"
    when: windowShown
    visible: true
    width: 900
    height: 700
    SearchModel {
        id: model
        property int polls: 0
        property var menuEntries: []
        property var held: []
        property var released: []
        function releaseNavigation(token) {
            held = held.filter(value => value !== token)
            released = released.concat([{token:token, destination:Navigation.went.slice(), remaining:held.slice()}])
        }
        function menuJson(index) { return JSON.stringify(menuEntries) }
        function pollSelection() { polls++ }
    }
    SearchMenu { id: menu; search: model }
    SearchNavigation { search: model }
    SignalSpy { id: left; target: menu; signalName: "left" }

    function waiting() { return findChild(test, "searchSelectionWait") }
    function init() {
        model.selectionPending = false
        model.selectionError = ""
        model.requestedAction = ""
        model.polls = 0
        model.held = []
        model.released = []
        AddToPlaylist.lastAsked = ""
        Navigation.went = []
        left.clear()
    }
    function cleanup() {
        model.cancelSelection()
        waiting().close()
        menu.close()
        wait(0)
    }
    function test_loading_is_cancelable_and_does_not_open_an_empty_picker() {
        model.pickMenu()
        tryCompare(waiting(), "opened", true)
        tryVerify(() => model.polls > 0)
        compare(AddToPlaylist.lastAsked, "")
        keyClick(Qt.Key_Escape)
        tryCompare(model, "selectionPending", false)
        compare(AddToPlaylist.lastAsked, "")
    }
    function test_failure_is_visible_and_can_be_dismissed() {
        model.selectionError = "Fixture source failed"
        tryCompare(waiting(), "opened", true)
        verify(waiting().contentItem.children.length > 0)
        compare(AddToPlaylist.lastAsked, "")
        keyClick(Qt.Key_Escape)
        tryCompare(model, "selectionError", "")
    }
    function test_ready_transfers_to_the_global_picker_before_closing() {
        model.pickMenu()
        tryCompare(waiting(), "opened", true)
        model.playlistReady("[-42,-43]")
        compare(AddToPlaylist.lastAsked, "[-42,-43]")
        model.cancelSelection()
        tryCompare(waiting(), "opened", false)
        compare(AddToPlaylist.lastAsked, "[-42,-43]")
    }
    function test_native_and_local_favorites_keep_the_flyout_and_use_hearts() {
        model.menuEntries = [{act:"favorite",text:"Favorite"},{act:"unfavorite",text:"Unfavorite"},{act:"mark",on:true,kind:"album",key:"1",text:"Favorite"}]
        menu.show(1, 0, 0)
        tryCompare(menu, "opened", true)
        compare(menu.entries[0].glyph, Icons.heart)
        compare(menu.entries[1].glyph, Icons.heartFilled)
        compare(menu.entries[2].glyph, Icons.heart)
        menu.chosen(0)
        compare(model.requestedAction, "favorite")
        compare(left.count, 0)
        menu.chosen(1)
        compare(model.requestedAction, "unfavorite")
        compare(left.count, 0)
    }
    function test_navigation_returns_from_the_model_before_changing_the_route() {
        model.navigated("artist", "Selected artist", 0, "single")
        compare(Navigation.went.length, 0)
        tryCompare(Navigation, "went", ["artist", "Selected artist"])
    }    function test_two_pending_navigations_release_only_their_own_token_after_the_menu_closes() {
        const first = "9007199254740993"
        const second = "9007199254740994"
        model.held = [first, second]
        model.menuEntries = [{act:"open",text:"Open"}]
        menu.show(1, 0, 0)
        tryCompare(menu, "opened", true)
        model.navigated("artist", "First artist", 0, first)
        model.navigated("artist", "Second artist", 0, second)
        menu.close()
        model.releaseMenu()
        compare(model.held, [first, second])
        compare(model.released.length, 0)
        compare(Navigation.went.length, 0)
        tryCompare(model, "released", [
            {token:first, destination:["artist", "First artist"], remaining:[second]},
            {token:second, destination:["artist", "Second artist"], remaining:[]}
        ])
        compare(model.held.length, 0)
    }

}
