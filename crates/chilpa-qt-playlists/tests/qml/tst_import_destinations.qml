import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

// The destinations step offers Chilpa's own playlist, the library and the
// accounts that take writes; names what was read; and shows public only
// where an account can say so.
TestCase {
    id: test
    name: "ImportDestinations"
    when: windowShown
    visible: true
    width: 900
    height: 700
    readonly property var places: ({mixed: {name: "Chilpa playlist", detail: "Best", info: {chilpa: true}}, local: {name: "Your library", detail: "Only yours", info: {local: true}},
        "com.test.cloud": {name: "Cloud", detail: "Playlist in your Cloud account", info: {icon: ""}, writable: true},
        "com.test.read": {name: "Read", detail: "Playlist in your Read account", info: {icon: ""}, writable: false}})
    readonly property var sources: [{id: "com.test.cloud", name: "Cloud", writable: true}, {id: "com.test.read", name: "Read", writable: false}]
    readonly property var read: ({phase: "preview", preview: {name: "List", description: "From a friend", total: 42}, settings: {}})
    ImportDestinationsStep { id: step; width: 760; height: 600; places: test.places; sources: test.sources; status: test.read }
    function init() {
        step.restored = false
        step.selected = ["mixed"]
        step.status = test.read
    }
    function test_the_read_list_shows_its_name_and_size() {
        compare(findChild(step, "importPreviewName").text, "List")
        verify(findChild(step, "importPreviewSize").text.indexOf("42") >= 0)
    }
    // Chilpa's own playlist first and chosen; the library; accounts that take writes.
    function test_the_places_are_chilpa_the_library_and_writable_accounts() {
        verify(findChild(step, "importPlace_mixed").checked)
        verify(findChild(step, "importPlace_local") !== null)
        verify(findChild(step, "importPlace_com.test.cloud") !== null)
        compare(findChild(step, "importPlace_com.test.read"), null, "an account that takes no writes is no place")
        verify(step.ready)
    }
    function test_public_shows_only_with_an_account_chosen() {
        verify(!findChild(step, "importPublic").visible)
        findChild(step, "importPlace_com.test.cloud").toggled()
        verify(findChild(step, "importPublic").visible)
        compare(step.payload().targets, ["mixed", "com.test.cloud"])
        findChild(step, "importPlace_com.test.cloud").toggled()
        verify(!findChild(step, "importPublic").visible)
    }
    function test_nothing_chosen_is_not_ready() {
        findChild(step, "importPlace_mixed").toggled()
        verify(!step.ready)
    }
    function test_the_name_starts_as_the_lists_and_goes_with_the_match() {
        compare(step.payload().title, "List")
        compare(step.payload().description, "From a friend")
        compare(step.payload().folder, 0)
        compare(step.payload().public, false)
    }
    function test_targets_said_by_the_importer_are_restored_once() {
        step.status = ({phase: "ready", preview: {name: "List", total: 42}, targets: ["local"], settings: {}})
        compare(step.selected, ["local"])
        verify(step.matchesTargets(["local"]))
        verify(!step.matchesTargets(["mixed"]))
    }
    function test_the_first_songs_show_before_choosing_where_they_go() {
        step.status = ({phase: "preview", settings: {}, preview: {name: "List", total: 12,
            head: [{title: "One", artist: "Band"}, {title: "Two", artist: "", uploader: "a fan"}]}})
        const head = findChild(step, "importPreviewHead")
        verify(head.visible)
        verify(findChild(step, "importPreviewMore").text.indexOf("10") >= 0)
    }
}
