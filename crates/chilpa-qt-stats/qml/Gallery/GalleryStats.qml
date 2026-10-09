import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"
import "../Patterns"

// The stats page's pieces with a made-up week: the hero, the tiles, the
// charts, the facts, a page of the history and its pager.
Item {
    id: gallery
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the samples and their margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins
    readonly property int sampleWide: 980
    readonly property var week: ({
        range: "3 – 9 Oct 2026", plays: 536, plays_change: 25, listened: "31 h 12 min", listened_change: -4,
        series: [["Fri", 60, 41], ["Sat", 112, 70], ["Sun", 98, 88], ["Mon", 51, 60], ["Tue", 74, 49], ["Wed", 66, 72], ["Thu", 75, 48]].map(d => ({ label: d[0], tip: d[0], plays: d[1], prev: d[2] })),
        artists: { count: 42, change: 11, top: [{ name: "Mora Vale", plays: 64, art: "" }, { name: "Yoko Kanno", plays: 41, art: "" }, { name: "May'n", plays: 33, art: "" }] },
        albums: { count: 57, change: -8, top: [{ title: "Faded Lines", artist: "Mora Vale", plays: 40, art: "", id: 1 }] },
        tracks: { count: 210, change: 0, top: [{ title: "Lion", artist: "May'n", plays: 12, art: "" }] },
        hours: [2, 1, 0, 0, 0, 0, 1, 4, 9, 12, 14, 10, 8, 9, 11, 13, 16, 20, 26, 30, 34, 28, 17, 6],
        weekdays: [51, 74, 66, 75, 60, 112, 98],
        genres: [{ name: "Anime", plays: 120 }, { name: "Jazz", plays: 64 }, { name: "Rock", plays: 40 }],
        decades: [{ decade: "1990", plays: 40 }, { decade: "2000", plays: 220 }, { decade: "2010", plays: 180 }],
        facts: { per_day: 76.6, best_day: { label: "Sat 4 Oct", plays: 112 }, bit_perfect: 480, events: 600, skips: 31 }
    })

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        width: gallery.sampleWide
        spacing: Metrics.gap16
        StatsHero { width: parent.width; report: gallery.week }
        StatsTops { width: parent.width; report: gallery.week }
        StatsCharts { width: parent.width; report: gallery.week }
        StatsFacts { width: parent.width; report: gallery.week }
        HistoryRow { width: parent.width; listen: ({ clock: "08:12", title: "Lion", artist: "May'n", album: "Macross Frontier", played: "4:58", length: "4:58", quality: "16/44.1", bit_perfect: true }) }
        HistoryRow { width: parent.width; index: 1; listen: ({ clock: "07:58", title: "Northern Cross", artist: "May'n", album: "Macross Frontier", played: "0:31", length: "4:57", quality: "24/96", skipped: true }) }
        Pager { page: 4; pages: 12 }
        Row {
            spacing: Metrics.gap16
            ChangeBadge { change: 25 }
            ChangeBadge { change: -8 }
            ChangeBadge { change: 0 }
        }
        Row {
            spacing: Metrics.gap16
            StatsCard { width: Charts.chartMin; height: Charts.chartTall; title: "Listening clock"; note: "Most at 20:00"; ListeningClock { anchors.fill: parent; hours: gallery.week.hours } }
            StatsCard { width: Charts.chartMin; height: Charts.chartTall; title: "Days"; ColumnBars { anchors.fill: parent; points: gallery.week.weekdays.map((v, i) => ({ label: "D" + i, value: v })) } }
            StatsCard { width: Charts.chartMin; height: Charts.chartTall; title: "Genres"; RankBars { width: parent.width; points: gallery.week.genres.map(g => ({ label: g.name, value: g.plays })) } }
        }
        PeriodBars { width: parent.width; height: Charts.chartTall; points: gallery.week.series }
        Row {
            spacing: Metrics.gap16
            TopTile { width: Charts.tileMin; height: Charts.tileTall; kind: "artists"; title: "Artists"; numbers: gallery.week.artists }
            RankRow { width: Charts.tileMin; rank: 2; title: "Yoko Kanno"; detail: "Macross Frontier"; plays: 41; most: 64 }
            FactCard { width: Charts.factMin; height: Charts.factTall; glyph: Icons.calendar; value: "Sat 4 Oct"; label: "Busiest day" }
        }
    }
}
