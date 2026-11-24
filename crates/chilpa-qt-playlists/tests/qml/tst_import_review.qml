import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

// The review lists every song against its destination on one line, with
// what was found, from where, how surely, and which source failed.
TestCase {
    id: test
    name: "ImportReview"
    when: windowShown
    visible: true
    width: 900
    height: 700
    readonly property var places: ({mixed: {name: "Chilpa playlist", info: {chilpa: true}}, local: {name: "Your library", info: {local: true}},
        "com.test.cloud": {name: "Cloud", info: {icon: ""}}})
    readonly property var rows: [
        {index: 1, original: {title: "Song", artist: "Band"}, status: "matched", errors: {}, matches: {mixed: {source: "com.test.cloud", title: "Song", artist: "Band", quality: {codec: "flac", bits: 24, rate: 96000}, confidence: 0.98}}},
        {index: 2, original: {title: "Lost", artist: "Band"}, status: "unmatched", errors: {}, matches: {}},
        {index: 3, original: {title: "Late", artist: "Band"}, status: "error", errors: {"com.test.cloud": "the plugin is not running"}, matches: {}}]
    readonly property var ready: ({phase: "ready", targets: ["mixed"], rows: test.rows, row_count: 3})
    ImportReviewStep { id: step; width: 760; height: 600; places: test.places; status: test.ready }
    function init() { step.status = test.ready; step.filter = "all"; step.offset = 0 }
    function row(index) {
        const list = findChild(step, "importRows")
        list.positionViewAtIndex(index, ListView.Beginning)
        return list.itemAtIndex(index)
    }
    function test_filters_without_counts_are_all_not_found_and_errors() {
        verify(findChild(step, "importFilter_all").active)
        verify(findChild(step, "importFilter_unmatched") !== null)
        verify(findChild(step, "importFilter_error") !== null)
        compare(findChild(step, "importFilter_found"), null, "the importer gives no counts")
    }
    function test_counts_from_the_importer_bring_their_filters() {
        step.status = ({phase: "ready", targets: ["mixed"], rows: test.rows, row_count: 3, counts: {all: 3, found: 1, options: 0, not_found: 1, error: 1}})
        verify(findChild(step, "importFilter_found").text.indexOf("1") >= 0)
        verify(findChild(step, "importFilter_options") !== null)
    }
    function test_a_filter_asks_its_first_page() {
        let asked = []
        const take = (filter, offset) => asked = [filter, offset]
        step.asked.connect(take)
        findChild(step, "importFilter_unmatched").clicked()
        step.asked.disconnect(take)
        compare(asked, ["unmatched", 0])
    }
    function test_a_row_says_what_was_found_from_where_and_how_surely() {
        const found = findChild(row(0), "importFound")
        verify(found.text.indexOf("98%") >= 0, found.text)
        verify(found.text.indexOf("FLAC") >= 0, found.text)
        compare(findChild(row(1), "importFound").text, "Not found")
    }
    function test_a_source_that_failed_is_named() {
        compare(findChild(row(2), "importTrouble").text, "Cloud did not answer")
    }
    function test_progress_shows_while_matching() {
        verify(!findChild(step, "importProgress").visible)
        step.status = ({phase: "matching", targets: ["mixed"], rows: [], row_count: 0, completed: 5, total_work: 10, current: "Band — Song"})
        verify(findChild(step, "importProgress").visible)
    }
    function test_pages_show_only_past_one() {
        verify(!findChild(step, "importPager").visible)
        step.status = ({phase: "ready", targets: ["mixed"], rows: test.rows, row_count: 450})
        verify(findChild(step, "importPager").visible)
    }
    readonly property var picked: ({phase: "ready", targets: ["mixed"], row_count: 1, counts: {all: 1, found: 1, options: 1, not_found: 0, error: 0},
        rows: [{index: 4, original: {title: "Song", artist: "Band"}, status: "matched", errors: {},
            matches: {mixed: {source: "com.test.cloud", ref: "c:1", title: "Song", confidence: 0.98}},
            alternatives: {mixed: [{source: "local", ref: "l:1", title: "Song", confidence: 0.95},
                {source: "com.test.cloud", ref: "c:2", title: "Song (Live)", version: "Live", confidence: 0.9}]}}]})
    // Other options show their count; one picked goes back as the song's choice.
    function test_a_row_offers_its_options_and_a_pick_is_sent() {
        step.status = test.picked
        const more = findChild(row(0), "importOptions")
        verify(more.visible)
        verify(more.text.indexOf("2") >= 0, more.text)
        more.clicked()
        const menu = findChild(step, "importOptionsMenu")
        tryCompare(menu, "opened", true)
        compare(menu.entries.length, 3, "the choice and its two options")
        verify(menu.entries[0].checked)
        verify(menu.entries[2].text.indexOf("Live") >= 0, menu.entries[2].text)
        let picked = null
        const take = (index, target, source, reference) => picked = [index, target, source, reference]
        step.chose.connect(take)
        menu.chosen(2)
        step.chose.disconnect(take)
        compare(picked, [4, "mixed", "com.test.cloud", "c:2"])
        menu.close()
    }
    // A source that failed on every song is said once, not on each row.
    function test_a_source_down_everywhere_is_said_once() {
        step.status = ({phase: "ready", targets: ["mixed"], rows: test.rows, row_count: 3, counts: {all: 3, found: 1, options: 0, not_found: 2, error: 3},
            source_errors: {"com.test.cloud": 3}})
        const down = findChild(step, "importSourceDown")
        verify(down.visible)
        verify(down.text.indexOf("Cloud") >= 0, down.text)
        compare(findChild(row(2), "importTrouble").text, "", "the row does not repeat it")
    }
    function test_an_upload_without_a_known_artist_shows_its_channel() {
        step.status = ({phase: "ready", targets: ["mixed"], row_count: 1,
            rows: [{index: 1, original: {title: "Theme", artist: "", uploader: "a fan"}, status: "unmatched", errors: {}, matches: {}}]})
        verify(findChild(row(0), "importSong").text.indexOf("a fan") >= 0)
    }
}
