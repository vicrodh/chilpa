import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"

TestCase {
    name: "SearchNavigation"
    QtObject {
        id: model
        property bool inside: false
        property bool releasedDuringSignal: false
        property var released: []
        signal navigated(string kind, string target, int id, string token)
        function act(id, token) {
            inside = true
            navigated("screen", "album", id, token)
            inside = false
        }
        function releaseNavigation(token) {
            releasedDuringSignal = releasedDuringSignal || inside
            released.push(token)
        }
    }
    SearchNavigation { id: navigation; search: model }
    function init() {
        navigation.flush()
        Navigation.page = ""
        Navigation.selectedId = 0
        model.released = []
        model.releasedDuringSignal = false
    }
    function test_the_caller_flushes_immediately_after_the_invokable_returns() {
        model.act(7, "first")
        compare(Navigation.page, "")
        compare(model.released.length, 0)
        navigation.flush()
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, 7)
        compare(model.released, ["first"])
        compare(model.releasedDuringSignal, false)
        wait(0)
        compare(model.released.length, 1, "I leave the deferred fallback with nothing to dispatch twice")
    }
    function test_unknown_consumers_keep_the_deferred_fallback_and_each_hold() {
        model.act(7, "first")
        model.act(8, "second")
        compare(model.released.length, 0)
        tryCompare(Navigation, "selectedId", 8)
        compare(model.released, ["first", "second"])
        compare(model.releasedDuringSignal, false)
    }
}
