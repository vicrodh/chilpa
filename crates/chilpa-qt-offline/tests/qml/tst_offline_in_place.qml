import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../../chilpa-qt/qml/Components"
import "../../qml/Components"

// A track's offline state stays on its own row, wherever the listener is:
// the ring around its menu button in each state, the arc only on the row
// whose file is on its way, the same place and size in every state, a row
// that is reused for another track showing that track's state, and the
// line over a header's rule that keeps the rule's height.
TestCase {
    id: test
    name: "OfflineInPlace"
    when: windowShown
    visible: true
    width: 420
    height: 400

    Component { id: endFactory; TrackRowEnd { x: 300; y: 20; height: Metrics.rowCompact } }
    Component { id: queueFactory; QueueRow { x: 20; y: 120; width: 380; index: 1; title: "Orangenlied"; artist: "Annenmaykantereit"; duration: "4:00"; playing: false; art: ""; trackId: 5 } }
    Component { id: lineFactory; OfflineLine { x: 20; y: 300; width: 380 } }

    function init() {
        Offline.marks = ({})
        Offline.jobs = ({})
        Pulse.held = 0
        ++Offline.rev
    }
    function mark(state, fraction, error) {
        Offline.marks = {5: {state: state, fraction: fraction === undefined ? -1 : fraction, error: error || ""}}
        ++Offline.rev
    }
    function shownMark(root, name) {
        const loader = findChild(root, name)
        return loader && loader.active ? loader.item : null
    }

    function test_each_state_draws_around_the_menu_button_without_moving_it_data() {
        return [
            {tag: "queued", state: "queued", ring: true, color: "textTertiary", arc: false},
            {tag: "arriving, size unknown", state: "downloading", fraction: -1, ring: false, arc: true, sweeping: true},
            {tag: "arriving, size known", state: "downloading", fraction: 0.6, ring: false, arc: true, sweeping: false},
            {tag: "verifying", state: "verifying", fraction: 0.6, ring: false, arc: true, sweeping: true},
            {tag: "removing", state: "removing", ring: false, arc: true, sweeping: true},
            {tag: "kept", state: "ready", ring: true, color: "accent", arc: false},
            {tag: "failed", state: "failed", ring: true, color: "statusNegative", arc: false}]
    }
    function test_each_state_draws_around_the_menu_button_without_moving_it(data) {
        const end = createTemporaryObject(endFactory, test, {trackId: 5})
        const width = end.width, height = end.height
        compare(shownMark(end, "trackOfflineMark"), null, "nothing offline, nothing drawn")
        mark(data.state, data.fraction)
        const found = shownMark(end, "trackOfflineMark")
        verify(found !== null)
        compare([end.width, end.height], [width, height], "the row's end keeps its size")
        compare([found.width, found.height], [Metrics.offlineRing, Metrics.offlineRing])
        const centre = found.mapToItem(end, found.width / 2, found.height / 2)
        compare([centre.x, centre.y], [Metrics.rowAction * 1.5, height / 2], "centred on the menu button")
        compare(findChild(found, "offlineRing").visible, data.ring)
        if (data.ring) compare(String(findChild(found, "offlineRing").border.color), String(Theme.color[data.color]))
        const arc = findChild(found, "offlineArc")
        compare(arc.active, data.arc)
        if (data.arc) {
            compare(arc.item.sweeping, data.sweeping)
            tryVerify(() => (Pulse.held > 0) === data.sweeping, 1000, "only a turning arc holds the clock")
            if (!data.sweeping) {
                compare(arc.item.angle, 216)
                verify(findChild(arc.item, "offlineArcLeft").visible, "past half a turn")
            }
        }
        mark("")
        tryCompare(findChild(end, "trackOfflineMark"), "active", false)
        tryCompare(Pulse, "held", 0)
    }

    function test_a_reused_row_shows_the_track_it_now_holds() {
        Offline.marks = {5: {state: "ready"}, 6: {state: "downloading", fraction: -1}}
        ++Offline.rev
        const end = createTemporaryObject(endFactory, test, {trackId: 5})
        compare(shownMark(end, "trackOfflineMark").phase, "ready")
        end.trackId = 7
        compare(findChild(end, "trackOfflineMark").active, false)
        end.trackId = 6
        compare(shownMark(end, "trackOfflineMark").phase, "downloading")
        end.trackId = 5
        compare(shownMark(end, "trackOfflineMark").phase, "ready", "the state lives with the track, not the row")
    }

    function test_an_arc_off_the_screen_turns_nothing() {
        mark("downloading", -1)
        const end = createTemporaryObject(endFactory, test, {trackId: 5, inViewport: false})
        compare(findChild(shownMark(end, "trackOfflineMark"), "offlineArc").active, false)
        compare(Pulse.held, 0)
    }

    function test_a_queued_track_shows_its_state_around_its_menu() {
        mark("failed", -1, "The server answered 500")
        const row = createTemporaryObject(queueFactory, test)
        const found = shownMark(row, "queueOfflineMark")
        verify(found !== null && findChild(found, "offlineRing").visible)
        compare(Offline.tip(5, Offline.rev), "The server answered 500")
        row.playing = true
        compare(findChild(row, "queueOfflineMark").active, false, "the playing row has no menu button to ring")
    }

    function test_a_selection_line_keeps_the_rule_height() {
        const line = createTemporaryObject(lineFactory, test, {kind: "album", selected: "[17]"})
        const fill = findChild(line, "offlineLineFill")
        compare(fill.visible, false)
        Offline.jobs = {"album[17]": 0.5}
        ++Offline.rev
        compare(fill.visible, true)
        compare(fill.width, line.width / 2)
        compare(line.height, Metrics.hairline, "nothing under the rule moves")
        Offline.jobs = {"album[17]": 0}
        ++Offline.rev
        compare(fill.width, Metrics.offlineRing, "a selection that just began shows")
        Offline.jobs = ({})
        ++Offline.rev
        compare(fill.visible, false)
    }

    // The window's one poll runs only while something is going.
    function test_the_window_reads_marks_only_while_work_goes() {
        const host = createTemporaryObject(Qt.createComponent("../../qml/Components/OfflineActivity.qml"), test)
        const poll = findChild(host, "offlineActivityPoll")
        compare(poll.running, false)
        Offline.active = true
        compare(poll.running, true)
        Offline.calls = []
        tryVerify(() => Offline.calls.some(call => call[0] === "refresh" && call[1] === false), 2000)
        Offline.active = false
        compare(poll.running, false)
        host.listening = false
        Offline.active = true
        compare(poll.running, false, "the gallery's copy listens to nothing")
        Offline.active = false
    }
}
