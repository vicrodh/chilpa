import QtQuick
import QtTest
import app.chilpa
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Theme"
import "../../qml/Screens"
import "../../qml/Components"

// The stats page and the history: the kind of period chosen and kept, the
// arrows, the calendar and Today walking through periods, what the report
// shows, the exports, the history's pages, its search, its columns and
// its density.
TestCase {
    name: "Stats"
    when: windowShown
    visible: true
    width: 1200
    height: 900

    Component { id: statsPage; StatsScreen { width: 1200; height: 900 } }
    Component { id: historyPage; HistoryScreen { width: 1200; height: 900 } }

    function init() {
        Stats.asked.length = 0
        Stats.exports.length = 0
        SettingsModel.preferences = ({})
        SettingsModel.density = "rich"
    }

    function test_a_week_by_default_compared_with_the_last_and_the_kind_is_kept() {
        const page = createTemporaryObject(statsPage, this)
        compare(page.kind, "week")
        compare(findChild(page, "heroPlays").text, "10")
        compare(findChild(page, "statsComparison").text, "This week vs Last week")
        compare(findChild(page, "statsDay").text, "week now", "the calendar's button says the period")
        findChild(page, "statsPeriod").activated(3)
        compare(page.kind, "year")
        compare(SettingsModel.kept("ui.stats.period"), "year", "the kind is kept for next time")
        compare(Stats.asked[Stats.asked.length - 1], "year:")
    }

    function test_the_arrows_and_the_calendar_walk_and_today_comes_back() {
        const page = createTemporaryObject(statsPage, this)
        const later = findChild(page, "statsLater")
        const today = findChild(page, "statsToday")
        verify(!later.enabled, "nothing after today")
        verify(!today.visible, "today needs no way back to today")
        mouseClick(findChild(page, "statsEarlier"))
        compare(page.anchor, "2026-10-01")
        compare(findChild(page, "heroPlays").text, "11")
        compare(findChild(page, "statsComparison").text, "vs before")
        verify(later.enabled)
        verify(today.visible)
        compare(findChild(page, "statsPeriod").options[0].text, "Day", "a day gone by is no longer today")
        // Shown just now: its row places it on the next polish.
        tryVerify(() => !isPolishScheduled(today.parent))
        mouseClick(today)
        compare(page.anchor, "")
        verify(!today.visible)
        const calendar = findChild(page, "statsCalendar")
        compare(calendar.latest, "2026-10-09", "nothing after today can be picked")
        calendar.picked("2026-09-15")
        compare(page.anchor, "2026-09-15")
        compare(Stats.asked[Stats.asked.length - 1], "week:2026-09-15")
        findChild(page, "statsPeriod").activated(4)
        verify(!findChild(page, "statsEarlier").visible, "all of it has nothing before it")
    }

    function test_the_history_and_the_picture_go_out() {
        const page = createTemporaryObject(statsPage, this)
        findChild(page, "statsHead").tableAsked()
        compare(Stats.exports, ["csv"])
        tryCompare(findChild(page, "statsNote"), "text", "130 listens saved")
        page.picture()
        verify(page.picturing, "the caption and the ground are on while the picture is taken")
        tryCompare(page, "picturing", false)
    }

    function test_the_history_pages_and_its_size() {
        const page = createTemporaryObject(historyPage, this)
        compare(findChild(page, "historyTotal").text, "130 listens")
        const pager = findChild(page, "historyPager")
        compare(pager.pages, 7)
        verify(pager.mapToItem(page, 0, pager.height).y <= page.height - Metrics.playerSpace, "the pager stays clear of the player bar")
        compare(pager.shown, [1, 2, 0, 7], "the first, its neighbour, a gap and the last")
        mouseClick(findChild(pager, "page2"))
        compare(page.at, 2)
        compare(pager.shown, [1, 2, 3, 0, 7])
        findChild(page, "historySize").activated(2)
        compare(page.perPage, 100)
        compare(SettingsModel.kept("ui.history.perPage"), "100", "the size is kept")
        compare(page.at, 1, "a new size starts from the first page")
        compare(pager.pages, 2)
        mouseClick(findChild(pager, "pagerForward"))
        compare(page.at, 2)
        verify(!findChild(pager, "pagerForward").enabled, "nothing after the last page")
    }

    function test_the_search_keeps_what_matches_from_the_first_page() {
        const page = createTemporaryObject(historyPage, this)
        mouseClick(findChild(findChild(page, "historyPager"), "page2"))
        findChild(page, "historySearch").text = "mora"
        compare(page.query, "mora")
        compare(page.at, 1)
        compare(findChild(page, "historyTotal").text, "3 listens")
        compare(findChild(page, "historyRow0").listen.title, "Match 1")
    }

    function test_the_columns_stay_put_whatever_a_row_holds() {
        const page = createTemporaryObject(historyPage, this)
        const plain = findChild(page, "historyRow0")
        const skipped = findChild(page, "historyRow1")
        verify(skipped.listen.skipped && !plain.listen.skipped)
        const at = (row, name) => findChild(row, name).mapToItem(page, 0, 0).x
        compare(at(skipped, "historyQuality"), at(plain, "historyQuality"), "a skip does not push the quality over")
        compare(at(skipped, "historyTrack") + findChild(skipped, "historyTrack").width, at(plain, "historyTrack") + findChild(plain, "historyTrack").width)
        compare(plain.height, Charts.historyRow)
        SettingsModel.density = "compact"
        compare(plain.height, Metrics.rowCompact, "compact rows like the track lists")
        compare(at(skipped, "historyQuality"), at(plain, "historyQuality"))
    }
}
