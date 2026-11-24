import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

// The summary has a card per destination with what it got and a way to
// open it, and the songs left out folded away.
TestCase {
    id: test
    name: "ImportSummary"
    when: windowShown
    visible: true
    width: 900
    height: 700
    readonly property var places: ({mixed: {name: "Chilpa playlist", info: {chilpa: true}}, "com.test.cloud": {name: "Cloud", info: {icon: ""}}})
    ImportSummaryStep {
        id: step; width: 760; height: 600; places: test.places
        status: ({phase: "done", row_count: 2,
            rows: [{index: 3, original: {title: "Lost", artist: "Band"}, matches: {}, errors: {}}, {index: 9, original: {title: "Gone", artist: "Band"}, matches: {}, errors: {}}],
            outputs: [{target: "mixed", written: 40, unmatched: 2, duplicates: 0, error: "", parts: [{ref: "17", title: "List"}]},
                {target: "com.test.cloud", written: 10, unmatched: 0, duplicates: 1, error: "The account refused the playlist.", parts: [{ref: "p:1", title: "List"}]}]})
    }
    function init() { step.missingShown = false }
    function card(index) { return findChild(step, "importResult_" + index) }
    function test_each_destination_says_what_it_got() {
        compare(findChild(card(0), "importResultName").text, "Chilpa playlist")
        compare(findChild(card(0), "importResultCounts").text, "40 songs added · 2 not found")
        compare(findChild(card(1), "importResultCounts").text, "10 songs added · 1 duplicate")
        verify(findChild(card(1), "importResultError").visible)
        verify(!findChild(card(0), "importResultError").visible)
    }
    function test_open_names_the_destination_and_its_part() {
        let opened = null
        const take = (output, part) => opened = [output.target, part.ref]
        step.opened.connect(take)
        findChild(card(0), "importOpen").clicked()
        step.opened.disconnect(take)
        compare(opened, ["mixed", "17"])
    }
    function test_the_songs_left_out_fold_away() {
        const missing = findChild(step, "importMissing")
        verify(missing.text.indexOf("2") >= 0)
        verify(!findChild(step, "importMissingRows").visible)
        missing.clicked()
        verify(findChild(step, "importMissingRows").visible)
    }
}
