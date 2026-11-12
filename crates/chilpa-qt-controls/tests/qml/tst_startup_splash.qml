import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

// The window's cover while it starts: the logo over the name, and a bar for
// each thing read before the window takes a click, the plugins and the
// library's first reading, each saying what it is. It goes only when every
// one is done, a later reading does not bring it back, and a reading that
// never answers does not keep the window shut.
TestCase {
    id: test
    name: "StartupSplash"; when: windowShown && TextAudit.ready; visible: true; width: 900; height: 700
    Component { id: factory; StartupSplash { anchors.fill: parent } }
    property var before

    function initTestCase() { before = Sources.state }
    function init() {
        I18n.choose("en")
        Sources.state = Object.assign({}, before, { starting: true, startup: { total: 2, done: 1, name: "Radio" } })
        Library.rev = 0
        Library.busy = true
    }
    function cleanupTestCase() { Sources.state = before; Library.busy = false; I18n.choose("en") }
    function tasks(splash) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === "startupTask") out.push(child); walk(child) } }
        walk(splash)
        return out
    }
    function up() { Sources.state = Object.assign({}, before, { starting: false }) }

    function test_the_logo_tops_the_splash_while_the_window_starts() {
        const splash = createTemporaryObject(factory, test)
        tryCompare(splash, "opacity", 1)
        const logo = findChild(splash, "splashLogo")
        verify(logo.visible)
        tryCompare(logo, "status", Image.Ready)
        compare(logo.y, 0, "the logo is the first thing in the column, over the name and the bars")
    }

    function test_each_task_has_its_bar_and_says_what_it_loads() {
        const splash = createTemporaryObject(factory, test)
        const bars = tasks(splash)
        compare(bars.length, 2, "the plugins and the library; a server's sync has none")
        compare(findChild(bars[0], "startupTaskText").text, "Starting Radio…")
        tryVerify(() => Math.abs(findChild(bars[0], "startupTaskFill").width - bars[0].width / 2) < 1, 2000, "one plugin of two: half the bar")
        compare(findChild(bars[1], "startupTaskText").text, "Reading the library…")
        verify(findChild(bars[1], "startupTaskSweep").visible, "how far the library's reading is cannot be told")
        Library.busy = false
        tryVerify(() => Math.abs(findChild(bars[1], "startupTaskFill").width - bars[1].width) < 1, 2000, "read, its bar fills")
    }

    function test_the_cover_stays_until_every_task_is_done() {
        const splash = createTemporaryObject(factory, test)
        up()
        wait(Motion.poll * 2)
        verify(splash.waiting, "the plugins are up and the library is still being read")
        compare(splash.opacity, 1)
        Library.busy = false
        tryCompare(splash, "waiting", false)
        tryCompare(splash, "opacity", 0)
    }

    function test_a_first_answer_counts_while_a_newer_reading_runs() {
        const splash = createTemporaryObject(factory, test)
        up()
        Library.rev = 1
        tryCompare(splash, "waiting", false, 2000, "the library's first answer arrived, a newer one still on its way")
    }

    function test_a_later_reading_does_not_bring_the_cover_back() {
        const splash = createTemporaryObject(factory, test)
        up()
        Library.busy = false
        tryCompare(splash, "waiting", false)
        Library.busy = true
        wait(Motion.poll)
        verify(!splash.waiting, "a sync's writes are read again behind the window, not under a cover")
    }

    function test_a_reading_that_never_answers_does_not_keep_the_window_shut() {
        const splash = createTemporaryObject(factory, test, { limit: 200 })
        up()
        ignoreWarning(/startup: still reading after 200 ms/)
        tryCompare(splash, "waiting", false, 2000)
    }

    // The lines are small: each one fits its bar in every language, also a
    // tenth wider than Inter sets them.
    function test_the_lines_fit_data() { return TextAudit.cases() }
    function test_the_lines_fit(data) {
        I18n.choose(data.language)
        Sources.state = Object.assign({}, before, { starting: true, startup: { total: 12, done: 3, name: "Playlist Import (Hybrid)" } })
        const splash = createTemporaryObject(factory, test)
        const found = TextAudit.check(test, splash, data.scale)
        compare(found.length, 0, found.join("\n"))
        for (const bar of tasks(splash))
            verify(!findChild(bar, "startupTaskText").truncated, "a line is never cut")
    }
}
