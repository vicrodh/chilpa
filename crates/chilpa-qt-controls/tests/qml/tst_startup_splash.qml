import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

// The window's cover while it starts: the logo over the name, and a bar for
// each thing read before the window takes a click, the plugins and the
// library's first reading, each saying what it is. It goes only when every
// one is done, a later reading does not bring it back, and a reading that
// never answers does not keep the window shut. A library older than this
// Chilpa is upgraded first, under its own bar, as long as it takes; if it
// fails the cover says why and stays.
TestCase {
    id: test
    name: "StartupSplash"; when: windowShown && TextAudit.ready; visible: true; width: 900; height: 700
    Component { id: factory; StartupSplash { anchors.fill: parent } }
    property var before

    function initTestCase() { before = Sources.state }
    function init() {
        I18n.choose("en")
        LibraryUpgrade.state = { active: false }
        LibraryUpgrade.retried = 0
        LibraryUpgrade.left = 0
        Sources.state = Object.assign({}, before, { starting: true, startup: { total: 2, done: 1, name: "Radio" } })
        Library.rev = 0
        Library.busy = true
    }
    function cleanupTestCase() { Sources.state = before; Library.busy = false; LibraryUpgrade.state = { active: false }; I18n.choose("en") }
    function tasks(splash) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName === "startupTask") out.push(child); walk(child) } }
        walk(splash)
        return out
    }
    function up() { Sources.state = Object.assign({}, before, { starting: false }) }
    readonly property string note: "This happens once after an update and can take a few minutes with a large library. Chilpa opens by itself when it is done."
    function upgrading(said) {
        LibraryUpgrade.state = Object.assign({ active: true, failed: false, text: "Step 2 of 4: changing how the library is stored",
                                               progress: 0.5, note: test.note }, said || {})
    }
    function failing(said) {
        LibraryUpgrade.state = Object.assign({ active: true, failed: true, title: "The library could not be updated", message: "disk I/O error",
                                               detail: "Nothing was lost: the library is as it was before this step." }, said || {})
    }

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

    function test_nothing_to_upgrade_shows_nothing_of_it() {
        const splash = createTemporaryObject(factory, test)
        compare(tasks(splash).length, 2)
        verify(!findChild(splash, "upgradeNote").visible)
        verify(!findChild(splash, "upgradeFailure").visible)
        verify(LibraryUpgrade.refreshed > 0, "the cover asks")
    }

    function test_an_upgrade_is_the_first_bar_and_says_what_it_does() {
        upgrading()
        const splash = createTemporaryObject(factory, test)
        let bars = tasks(splash)
        compare(bars.length, 3, "the upgrade, the plugins and the library")
        compare(findChild(bars[0], "startupTaskText").text, "Step 2 of 4: changing how the library is stored")
        tryVerify(() => Math.abs(findChild(bars[0], "startupTaskFill").width - bars[0].width / 2) < 1, 2000, "half of it done")
        compare(findChild(splash, "upgradeNote").text, test.note)
        verify(findChild(splash, "upgradeNote").visible)
        up()
        Library.busy = false
        wait(Motion.poll * 2)
        verify(splash.waiting, "nothing else counts while the library is upgraded")
        verify(!splash.tasks[1].done, "the plugins start after it")
        verify(!splash.libraryRead, "what arrives while it is upgraded is not the library's answer")
        upgrading({ text: "Updating the library…", progress: -1 })
        bars = tasks(splash)
        compare(findChild(bars[0], "startupTaskText").text, "Updating the library…")
        verify(findChild(bars[0], "startupTaskSweep").visible, "before it says how far it is, the bar sweeps")
    }

    function test_an_upgrade_has_no_time_limit_and_the_library_is_read_after_it() {
        upgrading()
        const splash = createTemporaryObject(factory, test, { limit: 1000 })
        up()
        Library.busy = false
        wait(1500)
        verify(splash.waiting, "an upgrade is waited for whole")
        verify(!splash.given)
        // Once it is current the window reads the library again.
        Library.busy = true
        LibraryUpgrade.state = { active: false }
        wait(Motion.poll)
        verify(splash.waiting, "that reading is waited for")
        compare(tasks(splash).length, 2)
        Library.busy = false
        tryCompare(splash, "waiting", false, 2000)
    }

    function test_a_failed_upgrade_says_why_and_what_can_be_done() {
        failing({ detail: "Nothing was lost: the library is as it was before this step, and a copy from before the update is at /fixture/library.db.before-upgrade." })
        const splash = createTemporaryObject(factory, test, { limit: 200 })
        compare(tasks(splash).length, 0, "nothing is under way")
        const failure = findChild(splash, "upgradeFailure")
        verify(failure.visible)
        verify(!findChild(splash, "upgradeNote").visible)
        compare(findChild(failure, "upgradeFailureTitle").text, "The library could not be updated")
        compare(findChild(failure, "upgradeFailureMessage").text, "disk I/O error")
        compare(findChild(failure, "upgradeFailureDetail").text, "Nothing was lost: the library is as it was before this step, and a copy from before the update is at /fixture/library.db.before-upgrade.")
        up()
        Library.busy = false
        wait(600)
        verify(splash.waiting, "the window never opens on a library that failed its upgrade")
        mouseClick(findChild(failure, "upgradeRetry"))
        compare(LibraryUpgrade.retried, 1)
        mouseClick(findChild(failure, "upgradeQuit"))
        compare(LibraryUpgrade.left, 1)
    }

    // Each line of an upgrade, and its failure, fits the cover in every
    // language, also a tenth wider.
    function test_the_upgrade_lines_fit_data() { return TextAudit.cases() }
    function test_the_upgrade_lines_fit(data) {
        I18n.choose(data.language)
        const step = I18n.trArgs("Step {} of {}: {}", ["12", "12", I18n.tr("changing how the library is stored", I18n.rev)], I18n.rev)
        const ids = I18n.trArgs("Step {} of {}: {}", ["12", "12", I18n.tr("giving every track a number that is never used again", I18n.rev)], I18n.rev)
        const lines = [I18n.tr("Updating the library…", I18n.rev), I18n.tr("Saving a copy of the library first…", I18n.rev), step, ids,
                       I18n.tr("Checking the result…", I18n.rev), I18n.tr("Another Chilpa is updating this library. Waiting for it…", I18n.rev)]
        for (const line of lines) {
            upgrading({ text: line, note: I18n.tr("This happens once after an update and can take a few minutes with a large library. Chilpa opens by itself when it is done.", I18n.rev) })
            const splash = createTemporaryObject(factory, test)
            const found = TextAudit.check(test, splash, data.scale)
            compare(found.length, 0, line + "\n" + found.join("\n"))
            for (const bar of tasks(splash))
                verify(!findChild(bar, "startupTaskText").truncated, "a line is never cut")
        }
        failing({ title: I18n.tr("The library could not be updated", I18n.rev), message: "database or disk is full",
                  detail: I18n.trArgs("Nothing was lost: the library is as it was before this step, and a copy from before the update is at {}.",
                                      ["/home/listener/.local/share/chilpa/library.db.before-upgrade"], I18n.rev) })
        const splash = createTemporaryObject(factory, test)
        const found = TextAudit.check(test, splash, data.scale)
        compare(found.length, 0, found.join("\n"))
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
