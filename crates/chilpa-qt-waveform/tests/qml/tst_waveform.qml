import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.waveform
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"
import "../../qml/Theme"

// The waveform seek: it asks for the columns its width has room for,
// draws one bar per value in the colors of played, known and unknown,
// seeks where it is clicked and dragged, and reads the document again
// when the track changes.
TestCase {
    id: test
    name: "Waveform"
    when: windowShown
    visible: true
    width: 900
    height: 120

    WaveformSeek {
        id: bar
        x: 0; y: 40
        width: 600
        height: Metrics.waveformBand
        position: 30
        duration: 120
        onSeek: (seconds) => test.sought = seconds
    }
    property real sought: -1

    function fill(count, level) {
        const out = []
        for (let i = 0; i < count; ++i)
            out.push(level)
        return out
    }

    function init() {
        test.sought = -1
        Waveform.analyzed = 0.5
        Waveform.complete = false
        Waveform.values = fill(bar.columns, 0.8)
        Waveform.lows = fill(bar.columns, 0.4)
        waitForRendering(bar)
    }

    function test_the_width_sets_the_columns_and_the_bridge_hears_it() {
        compare(bar.columns, Math.ceil(600 / Bars.pixelsPerColumn))
        compare(Waveform.columns, bar.columns)
        bar.width = 30
        tryCompare(bar, "columns", Bars.columnsMin, 1000, "never fewer than the least")
        compare(Waveform.columns, Bars.columnsMin)
        bar.width = 600
        tryCompare(bar, "columns", Math.ceil(600 / Bars.pixelsPerColumn))
    }

    // A panel sliding moves the width every frame: the bars laid out stretch
    // to it, none is rebuilt, and the columns are asked for once it rests.
    function test_a_moving_width_stretches_the_bars_and_asks_once_it_rests() {
        const laid = findChild(bar, "waveformBars")
        const first = laid.children.find(child => child.objectName === "waveformBar")
        for (const width of [590, 575, 560, 548, 540]) {
            bar.width = width
            wait(10)
        }
        compare(bar.columns, Math.ceil(600 / Bars.pixelsPerColumn), "the columns wait for the width to rest")
        verify(laid.children.includes(first), "no bar was rebuilt while it moved")
        compare(laid.width * laid.transform[0].xScale, 540, "the bars reach the new width")
        tryCompare(bar, "columns", Math.ceil(540 / Bars.pixelsPerColumn))
        compare(Waveform.columns, bar.columns)
        bar.width = 600
        tryCompare(bar, "columns", Math.ceil(600 / Bars.pixelsPerColumn))
        Waveform.values = fill(bar.columns, 0.8)
        Waveform.lows = fill(bar.columns, 0.4)
    }

    function test_one_bar_per_value_in_the_colors_of_played_known_and_unknown() {
        waitForRendering(bar)
        const bars = []
        const laid = findChild(bar, "waveformBars")
        for (let i = 0; i < laid.children.length; ++i) {
            const child = laid.children[i]
            if (child.objectName === "waveformBar")
                bars.push(child)
        }
        compare(bars.length, bar.columns)
        bars.sort((a, b) => a.x - b.x)
        compare(bars[0].color, Theme.color.accent, "the first quarter is played")
        compare(bars[Math.floor(bar.columns * 0.4)].color, Theme.color.textSecondary, "the first half is known")
        compare(bars[bar.columns - 1].color, Theme.color.strokeHairline, "the rest is not yet")
        verify(bars[1].x > bars[0].x && bars[1].x - bars[0].x <= Bars.pixelsPerColumn + 1, "one slot apart")
        verify(bars[0].height > Bars.barLeast && bars[0].height <= bar.height, "a bar within the height: " + bars[0].height)
        verify(bar.height / 2 - bars[0].y > bars[0].y + bars[0].height - bar.height / 2, "it swings further up than down")
    }

    function test_no_document_draws_no_bars() {
        Waveform.values = []
        Waveform.lows = []
        waitForRendering(bar)
        let count = 0
        const laid = findChild(bar, "waveformBars")
        for (let i = 0; i < laid.children.length; ++i)
            if (laid.children[i].objectName === "waveformBar")
                ++count
        compare(count, 0)
    }

    function test_a_click_seeks_there_and_a_drag_goes_where_it_is_let_go() {
        mouseClick(bar, bar.width / 2, bar.height / 2)
        verify(Math.abs(test.sought - 60) < 1, "the middle: " + test.sought)
        mousePress(bar, bar.width / 4, bar.height / 2)
        mouseMove(bar, bar.width * 3 / 4, bar.height / 2)
        verify(Math.abs(bar.shown - 90) < 1, "the drag shows where it is: " + bar.shown)
        mouseRelease(bar, bar.width * 3 / 4, bar.height / 2)
        verify(Math.abs(test.sought - 90) < 1, "and goes there when let go: " + test.sought)
    }

    function test_a_new_track_reads_the_document_again() {
        const before = Waveform.refreshes
        Player.path = "/another.flac"
        tryVerify(() => Waveform.refreshes > before, 1000)
    }
}
