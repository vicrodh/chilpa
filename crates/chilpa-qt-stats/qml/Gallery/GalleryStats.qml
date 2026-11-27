import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Theme"
import "../Components"
import "../Patterns"

// The stats page's pieces with a made-up week: the head, the hero, the
// tiles, the charts, the facts, the history's columns, rich and compact
// rows, its pager, and the calendar the head opens.
Item {
    id: gallery
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the samples and their margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins
    readonly property int sampleWide: 980
    readonly property var week: ({
        kind: "week", range: "3 – 9 Oct 2026", comparison: "This week vs Last week", anchor: "2026-10-09", now: "2026-10-09",
        today: true, current: true, earlier: "2026-09-26", later: "", plays: 536, plays_change: 25, listened: "31 h 12 min", listened_change: -4,
        series: [["Fri", 60, 41], ["Sat", 112, 70], ["Sun", 98, 88], ["Mon", 51, 60], ["Tue", 74, 49], ["Wed", 66, 72], ["Thu", 75, 48]].map(d => ({ label: d[0], tip: d[0], plays: d[1], prev: d[2] })),
        artists: { count: 42, change: 11, top: [{ name: "Mora Vale", plays: 64, art: "" }, { name: "Yoko Kanno", plays: 41, art: "" }, { name: "May'n", plays: 33, art: "" }] },
        albums: { count: 57, change: -8, top: [{ title: "Faded Lines", artist: "Mora Vale", plays: 40, art: "", id: 1 }] },
        tracks: { count: 210, change: 0, top: [{ title: "Lion", artist: "May'n", plays: 12, art: "" }] },
        hours: [2, 1, 0, 0, 0, 0, 1, 4, 9, 12, 14, 10, 8, 9, 11, 13, 16, 20, 26, 30, 34, 28, 17, 6],
        weekdays: [["Mon", 51], ["Tue", 74], ["Wed", 66], ["Thu", 75], ["Fri", 60], ["Sat", 112], ["Sun", 98]].map(d => ({ label: d[0], plays: d[1] })),
        genres: [{ name: "Anime", plays: 120 }, { name: "Jazz", plays: 64 }, { name: "Rock", plays: 40 }],
        decades: [{ decade: "1990", plays: 40 }, { decade: "2000", plays: 220 }, { decade: "2010", plays: 180 }],
        facts: { per_day: 76.6, best_day: { label: "Sat 4 Oct", plays: 112 }, bit_perfect: 480, events: 600, skips: 31 }
    })

    // Six weeks of a made-up year, busier as they go.
    readonly property var year: ({
        year: 2026, earlier: true, later: false, columns: 6, levels: 4, most: 12, plays: 240,
        rows: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        months: [{ label: "Jan", column: 0 }, { label: "Feb", column: 5 }],
        cells: Array.from({ length: 42 }, (_, i) => ({ date: "2026-01-" + String(i + 1).padStart(2, "0"), column: Math.floor(i / 7), row: i % 7, plays: i % 5 * 3, level: i % 5, later: i > 38, tip: "Day " + (i + 1) }))
    })
    readonly property var month: ({
        year: 2026, month: 10, title: "Your October 2026 recap", first_year: 2025, first_month: 3, now_year: 2026, now_month: 11,
        months: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"], any: true, empty: false,
        artists: [["Mora Vale", 90], ["Yoko Kanno", 71], ["May'n", 40], ["Aimer", 33], ["Faded Lines", 21]].map(a => ({ name: a[0], plays: a[1], minutes: a[1] * 3, art: "" })),
        albums: [["Faded Lines", 87], ["Macross Frontier", 50], ["Sun Dance", 31]].map(a => ({ title: a[0], plays: a[1], minutes: a[1] * 3, art: "", id: 1 })),
        tracks: [["A song with a name long enough to be drawn smaller", 31], ["Lion", 20]].map(a => ({ title: a[0], plays: a[1], minutes: a[1] * 4, art: "" })),
        minutes: 3030, plays: 868, per_day: 29.9, tracks_count: 345, albums_count: 205, artists_count: 145, new_tracks: 63, new_albums: 24, new_artists: 13
    })

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        width: gallery.sampleWide
        spacing: Metrics.gap16
        StatsHead { width: parent.width; report: gallery.week; kind: "week" }
        StatsHero { width: parent.width; report: gallery.week }
        StatsTops { width: parent.width; report: gallery.week }
        StatsCharts { width: parent.width; report: gallery.week }
        StatsFacts { width: parent.width; report: gallery.week }
        AnalysisInventoryRow { width: parent.width; entry: ({title:"Saved track",artist:"Mora Vale",album:"Faded Lines",standard:true,mood:true,standard_at:1780000000,mood_at:1780000000}); alternate:true }
        AnalysisRunRow { width: parent.width; entry: ({kind:"mood",processed:23,total:100,saved:22,failed:1,previous:400,error:"",folder:"",last:"Saved track"}); stamp:"2026-10-14 10:00"; status:"Analyzing" }
        HistoryHeader { width: parent.width }
        HistoryRow { width: parent.width; listen: ({ clock: "08:12", title: "Lion", artist: "May'n", album: "Macross Frontier", played: "4:58", length: "4:58", quality: "16/44.1", bit_perfect: true }) }
        HistoryRow { width: parent.width; index: 1; listen: ({ clock: "07:58", title: "Northern Cross", artist: "May'n", album: "Macross Frontier", played: "0:31", length: "4:57", quality: "24/96", skipped: true }) }
        HistoryRow { width: parent.width; index: 2; rich: false; listen: ({ clock: "07:51", title: "Diamond Crevasse", artist: "May'n", album: "Macross Frontier", played: "5:10", length: "5:10", quality: "16/44.1", bit_perfect: true }) }
        Pager { page: 4; pages: 12 }
        Row {
            spacing: Metrics.gap24
            DateRange { latest: "2026-10-09" }
            DateRange { from: "2026-10-01"; to: "2026-10-09"; latest: "2026-10-09" }
        }
        PillButton {
            id: pickerSample
            glyph: Icons.calendar
            text: "Pick a day"
            onClicked: picker.open()
            DatePicker { id: picker; y: pickerSample.height + Metrics.gap4; selected: "2026-10-04"; latest: "2026-10-09" }
        }
        Row {
            spacing: Metrics.gap16
            ChangeBadge { change: 25 }
            ChangeBadge { change: -8 }
            ChangeBadge { change: 0 }
        }
        Row {
            spacing: Metrics.gap16
            StatsCard { width: Charts.chartMin; height: Charts.chartTall; title: "Listening clock"; note: "Most at 20:00"; ListeningClock { anchors.fill: parent; hours: gallery.week.hours } }
            StatsCard { width: Charts.chartMin; height: Charts.chartTall; title: "Days"; ColumnBars { anchors.fill: parent; points: gallery.week.weekdays.map(d => ({ label: d.label, value: d.plays })) } }
            StatsCard { width: Charts.chartMin; height: Charts.chartTall; title: "Genres"; RankBars { width: parent.width; points: gallery.week.genres.map(g => ({ label: g.name, value: g.plays })) } }
        }
        PeriodBars { width: parent.width; height: Charts.chartTall; points: gallery.week.series }
        StatsHero { width: parent.width; report: gallery.week; chart: "mosaic"; mosaic: gallery.year }
        MosaicGrid { width: parent.width; mosaic: gallery.year; chosen: "2026-01-09" }
        Row {
            spacing: Metrics.gap24
            RecapPeriods { width: Charts.recapPeriods; recap: gallery.month; shownYear: 2026 }
            RecapCard { width: gallery.sampleWide - Charts.recapPeriods - Metrics.gap24; recap: gallery.month; timed: true }
        }
        RecapTop { width: parent.width; caption: "Top album"; leaders: gallery.month.albums }
        RecapFigure { width: Charts.recapCell; value: "3,030"; label: "Minutes" }
        Row {
            spacing: Metrics.gap16
            TopTile { width: Charts.tileMin; height: Charts.tileTall; kind: "artists"; title: "Artists"; numbers: gallery.week.artists }
            RankRow { width: Charts.tileMin; rank: 2; title: "Yoko Kanno"; detail: "Macross Frontier"; plays: 41; most: 64 }
            RankRow { width: Charts.tileMin; rank: 128; rankWidth: Metrics.gap24 + Metrics.gap8; title: "Lion"; detail: "May'n"; plays: 3; most: 64; note: "12 min" } // derived: a place in the hundreds
            FactCard { width: Charts.factMin; height: Charts.factTall; glyph: Icons.calendar; value: "Sat 4 Oct"; label: "Busiest day" }
        }
    }
}
