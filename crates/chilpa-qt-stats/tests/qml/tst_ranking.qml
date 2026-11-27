import QtQuick
import QtTest
import app.chilpa
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Theme"
import "../../qml/Screens"

// A ranking: the kind and the span its route says, its pages numbered from
// the whole ranking, the size kept, another kind or span from the first
// page, and where each entry opens.
TestCase {
    name: "Ranking"
    when: windowShown
    visible: true
    width: 1200
    height: 900

    Component { id: rankingOf; RankingScreen { width: 1200; height: 900 } }

    function init() {
        Stats.asked.length = 0
        SettingsModel.preferences = ({})
        Navigation.went = []
    }

    function test_the_route_says_the_kind_and_the_days() {
        const page = createTemporaryObject(rankingOf, this, { routeKey: "albums\u001f2026-10-01\u001f2026-10-09" })
        compare([page.kind, page.from, page.to], ["albums", "2026-10-01", "2026-10-09"])
        compare(Stats.asked[Stats.asked.length - 1], "ranking:albums:2026-10-01:2026-10-09")
        compare(findChild(page, "rankingTotal").text, "5 albums")
        compare(findChild(page, "rankingKind").current, 1)
        compare(findChild(page, "rankingRange").latest, "2026-10-09")
    }

    function test_pages_number_from_the_whole_ranking_and_the_size_is_kept() {
        const page = createTemporaryObject(rankingOf, this, { routeKey: "artists" })
        compare(findChild(page, "rankingTotal").text, "75 artists")
        compare(page.perPage, 50, "fifty a page at first")
        const pager = findChild(page, "rankingPager")
        compare(pager.pages, 2)
        mouseClick(findChild(pager, "page2"))
        compare(findChild(page, "rankingRow0").rank, 51)
        compare(findChild(page, "rankingRow0").note, "51 min", "with the time it was heard")
        findChild(page, "rankingSize").activated(2)
        compare(SettingsModel.kept("ui.ranking.perPage"), "100")
        compare(page.at, 1)
    }

    function test_another_kind_or_span_starts_from_the_first_page() {
        const page = createTemporaryObject(rankingOf, this, { routeKey: "artists" })
        mouseClick(findChild(findChild(page, "rankingPager"), "page2"))
        findChild(page, "rankingKind").activated(2)
        compare([page.kind, page.at], ["genres", 1])
        compare(findChild(page, "rankingTotal").text, "12 genres")
        mouseClick(findChild(findChild(page, "rankingPager"), "page1"))
        findChild(page, "rankingRange").chosen("2026-10-01", "")
        compare([page.from, page.to, page.at], ["2026-10-01", "", 1])
        compare(Stats.asked[Stats.asked.length - 1], "ranking:genres:2026-10-01:")
    }

    function test_each_entry_opens_where_it_belongs() {
        const artists = createTemporaryObject(rankingOf, this, { routeKey: "artists" })
        mouseClick(findChild(artists, "rankingRow0"))
        compare(Navigation.went, ["artist", "artists 1"])
        const albums = createTemporaryObject(rankingOf, this, { routeKey: "albums" })
        mouseClick(findChild(albums, "rankingRow1"))
        compare([Navigation.page, Navigation.selectedId], ["album", 2])
        const genres = createTemporaryObject(rankingOf, this, { routeKey: "genres" })
        mouseClick(findChild(genres, "rankingRow0"))
        compare(Navigation.went[0], "genres")
        compare(JSON.parse(Navigation.went[1]).name, "genres 1")
    }
}
