import QtQuick
import QtTest
import app.chilpa
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Screens"
import "../../qml/Components"

// The stats page and the history: the period chosen and walked back,
// what the report shows, the history's pages and how many it shows.
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
        SettingsModel.preferences = ({})
    }

    function test_a_week_by_default_and_the_period_is_kept() {
        const page = createTemporaryObject(statsPage, this)
        compare(page.period, "week")
        compare(findChild(page, "heroPlays").text, "10")
        compare(findChild(page, "statsRange").text, "week 0")
        findChild(page, "statsPeriod").activated(2)
        compare(page.period, "year")
        compare(SettingsModel.kept("ui.stats.period"), "year", "the period is kept for next time")
        compare(Stats.asked[Stats.asked.length - 1], "year:0")
    }

    function test_the_arrows_walk_back_and_stop_at_today() {
        const page = createTemporaryObject(statsPage, this)
        const later = findChild(page, "statsLater")
        verify(!later.enabled, "nothing after today")
        mouseClick(findChild(page, "statsEarlier"))
        compare(page.back, 1)
        compare(findChild(page, "heroPlays").text, "11")
        verify(later.enabled)
        mouseClick(later)
        compare(page.back, 0)
        findChild(page, "statsPeriod").activated(3)
        verify(!findChild(page, "statsEarlier").visible, "all of it has nothing before it")
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
}
