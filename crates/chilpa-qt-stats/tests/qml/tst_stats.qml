import QtQuick
import QtTest
import app.chilpa
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
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
    Component { id: recapPage; RecapScreen { width: 1200; height: 900 } }
    Component { id: narrowMosaic; MosaicGrid { width: 300; height: 160 } }
    Component { id: narrowRecap; RecapScreen { width: 620; height: 900 } }

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

    function test_the_chart_arrows_walk_the_periods_like_the_heads() {
        const page = createTemporaryObject(statsPage, this)
        compare(page.chart, "bars")
        verify(findChild(page, "heroBars"), "the bars are the chart at first")
        verify(!findChild(page, "heroMosaic"), "the mosaic is not built until it is chosen")
        compare(findChild(page, "chartCaption").text, "week now")
        const later = findChild(page, "chartLater")
        verify(!later.enabled, "nothing after today")
        mouseClick(findChild(page, "chartEarlier"))
        compare(page.anchor, "2026-10-01")
        compare(findChild(page, "heroPlays").text, "11")
        verify(later.enabled)
        mouseClick(later)
        compare(page.anchor, "2026-10-09")
        verify(!Stats.asked.some(asked => asked.startsWith("mosaic:")), "nobody asked for a mosaic nobody sees")
        findChild(page, "statsPeriod").activated(4)
        verify(!findChild(page, "chartEarlier").enabled, "all of it has nothing before it")
    }

    function test_the_mosaic_is_chosen_kept_and_walked_a_year_at_a_time() {
        const page = createTemporaryObject(statsPage, this)
        findChild(page, "chartKind").activated(1)
        compare(page.chart, "mosaic")
        compare(SettingsModel.kept("ui.stats.chart"), "mosaic", "the chart is kept for next time")
        const mosaic = findChild(page, "heroMosaic")
        verify(mosaic)
        tryVerify(() => !findChild(page, "heroBars"), 1000, "the bars go when the mosaic takes their place")
        compare(mosaic.cells.length, 3)
        compare(findChild(page, "chartCaption").text, "2026 · 10 plays")
        const earlier = findChild(page, "chartEarlier")
        const later = findChild(page, "chartLater")
        verify(earlier.enabled)
        verify(!later.enabled, "no year after this one")
        mouseClick(earlier)
        compare(page.yearsBack, 1)
        compare(page.anchor, "", "the report stays on its period")
        compare(Stats.asked[Stats.asked.length - 1], "mosaic::1")
        compare(findChild(page, "chartCaption").text, "2025 · 11 plays")
        verify(later.enabled)
        // A square chooses its day, and the mosaic goes back to that day's year.
        mosaic.dayChosen("2025-01-05")
        compare(page.anchor, "2025-01-05")
        compare(page.yearsBack, 0)
        verify(Stats.asked.indexOf("mosaic:2025-01-05:0") >= 0, "asked for the year of the day chosen")
        compare(mosaic.chosen, "2025-01-05")
        const again = createTemporaryObject(statsPage, this)
        compare(again.chart, "mosaic", "what was kept is the chart it opens with")
    }

    function test_a_square_under_the_pointer_is_the_one_a_click_chooses() {
        const page = createTemporaryObject(statsPage, this)
        findChild(page, "chartKind").activated(1)
        const mosaic = findChild(page, "heroMosaic")
        const field = findChild(mosaic, "mosaicField")
        tryVerify(() => mosaic.step > 1)
        // The second column's first row: 5 January.
        mouseMove(field, mosaic.step + mosaic.cell / 2, mosaic.cell / 2)
        tryCompare(mosaic, "hovered", 1)
        mouseClick(field, mosaic.step + mosaic.cell / 2, mosaic.cell / 2)
        compare(page.anchor, "2026-01-05")
        // A day still to come is no square: nothing to point at or choose.
        mouseMove(field, mosaic.step + mosaic.cell / 2, mosaic.step + mosaic.cell / 2)
        tryCompare(mosaic, "hovered", -1)
        // An empty place of the grid neither.
        mouseMove(field, mosaic.cell / 2, mosaic.cell / 2)
        compare(mosaic.hovered, -1)
    }

    // The tooltip lets go of the square's marker when the grid hides; shown again under the pointer, the square says itself again.
    function test_a_square_hidden_and_shown_again_under_the_pointer_says_itself() {
        const page = createTemporaryObject(statsPage, this)
        findChild(page, "chartKind").activated(1)
        const mosaic = findChild(page, "heroMosaic")
        const field = findChild(mosaic, "mosaicField")
        tryVerify(() => mosaic.step > 1)
        mouseMove(field, mosaic.step + mosaic.cell / 2, mosaic.cell / 2)
        tryCompare(mosaic, "hovered", 1)
        const marker = TipState.target
        verify(marker !== null, "the square asked for the tooltip")
        mosaic.visible = false
        compare(TipState.target, null, "a hidden grid says nothing")
        mosaic.visible = true
        compare(TipState.target, marker, "shown again under the still pointer, the square says itself again")
        // A menu that opens takes the tooltip away; the pointer moving within the same square brings it back.
        TipState.hide(TipState.target)
        mouseMove(field, mosaic.step + mosaic.cell / 2 + 1, mosaic.cell / 2 + 1)
        compare(TipState.target, marker, "the pointer moving within the same square brings it back")
        TipState.hide(marker)
    }

    function test_a_year_too_wide_for_its_room_scrolls_and_opens_on_its_last_week() {
        const weeks = 53
        const cells = Array.from({ length: weeks * 7 }, (_, i) => ({ date: "day" + i, column: Math.floor(i / 7), row: i % 7, plays: 1, level: 1, later: i >= 40 * 7, tip: "" }))
        const grid = createTemporaryObject(narrowMosaic, this, { mosaic: { columns: weeks, levels: 4, cells: cells, months: [], rows: [] } })
        const view = findChild(grid, "mosaicView")
        compare(grid.step, Charts.mosaicLeast + Charts.mosaicApart, "a square is never drawn smaller than its least")
        verify(view.contentWidth > view.width)
        verify(view.interactive)
        compare(grid.openColumn, 39, "the last week there is")
        tryCompare(view, "contentX", 40 * grid.step - view.width)
        // The day chosen is the week it opens on.
        grid.chosen = "day14"
        compare(grid.openColumn, 2)
        tryCompare(view, "contentX", 0)
        // A year that fits does not scroll.
        grid.width = 1200
        verify(!view.interactive)
        compare(view.contentX, 0)
    }

    function test_the_recap_opens_on_last_month_and_a_month_or_a_year_is_picked() {
        const page = createTemporaryObject(recapPage, this)
        compare(Stats.asked[Stats.asked.length - 1], "recap:0:0")
        compare(findChild(page, "recapTitle").text, "Your Sep 2026 recap")
        const periods = findChild(page, "recapPeriods")
        compare(findChild(periods, "recapYear").text, "2026")
        compare(findChild(periods, "recapMonth9").kind, "primary", "the month shown is the chosen one")
        verify(!findChild(periods, "recapMonth11").enabled, "a month still to come")
        verify(findChild(periods, "recapMonth10").enabled, "the month running can be looked at")
        verify(!findChild(periods, "recapYearLater").enabled)
        // Stepping the year only changes the months offered.
        mouseClick(findChild(periods, "recapYearEarlier"))
        compare(findChild(periods, "recapYear").text, "2025")
        compare(findChild(page, "recapTitle").text, "Your Sep 2026 recap", "nothing is asked until a month is picked")
        verify(!findChild(periods, "recapMonth2").enabled, "before the first listen")
        verify(!findChild(periods, "recapYearEarlier").enabled, "no year before the first listen's")
        compare(findChild(periods, "recapMonth9").kind, "secondary", "September of another year is not the one shown")
        mouseClick(findChild(periods, "recapMonth3"))
        compare(Stats.asked[Stats.asked.length - 1], "recap:2025:3")
        compare(findChild(page, "recapTitle").text, "Your Mar 2025 recap")
        findChild(periods, "recapMode").activated(1)
        compare(page.mode, "year")
        // Shown just now: their flow places them on the next polish.
        tryVerify(() => !isPolishScheduled(findChild(periods, "recapYear2026").parent))
        mouseClick(findChild(periods, "recapYear2026"))
        compare(Stats.asked[Stats.asked.length - 1], "recap:2026:0")
        compare(findChild(page, "recapTitle").text, "Your 2026 recap")
        compare(findChild(periods, "recapYear2026").kind, "primary")
        compare(findChild(periods, "recapYear2025").kind, "secondary")
    }

    function test_the_recap_names_five_of_each_and_nine_numbers() {
        const page = createTemporaryObject(recapPage, this)
        const artist = findChild(page, "recapArtist")
        compare(findChild(artist, "recapLead").text, "Artist 1")
        compare(findChild(artist, "recapLeadPlays").text, "50 plays")
        compare(artist.leaders.length, 5)
        const values = []
        const figures = findChild(page, "recapFigures")
        for (let i = 0; i < figures.children.length; i++) {
            if (figures.children[i].label !== undefined)
                values.push(figures.children[i].label + "=" + figures.children[i].value)
        }
        // Thousands are grouped as the system's language groups them.
        compare(values, ["Minutes=" + Number(3030).toLocaleString(Qt.locale(), "f", 0), "Tracks=345", "New tracks=63", "Plays=868", "Albums=205", "New albums=24", "Plays a day=29.9", "Artists=145", "New artists=13"])
        compare(figures.columns, 3)
        // The time each sounded shows when asked for, and that is kept.
        findChild(page, "recapTimed").toggled(true)
        compare(findChild(artist, "recapLeadPlays").text, "50 plays · 200 min")
        compare(SettingsModel.kept("ui.recap.timed"), "true")
        // A record opens its page; a track opens nothing.
        verify(findChild(page, "recapAlbum").clickable)
        verify(!findChild(page, "recapTrack").clickable)
    }

    function test_a_period_with_too_little_says_so_and_a_narrow_page_stacks() {
        const page = createTemporaryObject(recapPage, this)
        page.pick(2024, 0)
        verify(findChild(page, "recapNothing").visible)
        verify(!findChild(page, "recapTitle").visible)
        verify(findChild(page, "recapPeriods").visible, "the periods stay to choose another")
        const narrow = createTemporaryObject(narrowRecap, this)
        verify(narrow.stacked)
        const card = findChild(narrow, "recapCard")
        verify(card.y > 0, "the card goes under the periods")
        compare(card.x, 0)
        compare(findChild(narrow, "recapFigures").columns, 1)
    }

    function test_the_head_opens_the_recap() {
        const page = createTemporaryObject(statsPage, this)
        let asked = 0
        const head = findChild(page, "statsHead")
        head.recapAsked.connect(() => asked++)
        const recap = findChild(page, "openRecap")
        const history = findChild(page, "openHistory")
        const exporter = findChild(page, "statsExport")
        mouseClick(recap)
        compare(asked, 1)
        verify(history.x < recap.x && recap.x < exporter.x, "between the history and the export")
    }

    function test_the_history_and_the_picture_go_out() {
        const page = createTemporaryObject(statsPage, this)
        findChild(page, "statsHead").tableAsked()
        compare(Stats.exports, ["csv"])
        tryCompare(findChild(page, "statsNote"), "text", "130 listens saved")
        const signature = findChild(page, "statsSignature")
        verify(!signature.visible, "the window shows no footer")
        page.picture()
        verify(page.picturing, "the caption and the ground are on while the picture is taken")
        verify(signature.visible, "the picture is signed")
        compare(findChild(page, "statsSignatureText").text, "Stats generated by Chilpa - www.chilpa.app")
        compare(String(findChild(page, "statsSignatureLogo").source), "qrc:/qt/qml/app/chilpa/kit/assets/logo.png")
        tryCompare(page, "picturing", false)
        verify(!signature.visible)
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
