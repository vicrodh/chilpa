pragma Singleton
import QtQuick

// The stats bridge as the tests see it: the periods, dates and pages it
// was asked for, documents made from them, and the exports it was asked
// for.
QtObject {
    id: stats

    // What was asked, kept where a binding that calls `report` does not
    // depend on it.
    readonly property var asked: []
    readonly property var exports: []
    property int total: 130
    property var inventoryDocument: ({total:0,standard:0,mood:0,matched:0,tracks:[],runs:[],active:[]})
    function inventory(filter, query, offset, run) { return JSON.stringify(inventoryDocument) }

    signal saved(string note)

    function report(kind, anchor, firstDay, rev) {
        asked.push(kind + ":" + anchor)
        const past = anchor !== ""
        const bars = kind === "week" ? 7 : 3
        return JSON.stringify({
            kind: kind, anchor: past ? anchor : "2026-10-09", now: "2026-10-09", today: !past, current: !past,
            earlier: kind === "all" ? "" : "2026-10-01", later: past ? "2026-10-09" : "",
            range: kind + " " + (past ? anchor : "now"), comparison: past ? "vs before" : "This week vs Last week",
            plays: past ? 11 : 10, plays_change: past ? -10 : 25,
            listened: "1 h 2 min", listened_change: null,
            series: Array.from({ length: bars }, (_, i) => ({ label: "d" + i, tip: "day " + i, plays: i, prev: bars - i })),
            artists: { count: 2, change: 100, top: [{ name: "Mora Vale", plays: 6, art: "" }, { name: "Yoko Kanno", plays: 3, art: "" }] },
            albums: { count: 1, change: null, top: [{ title: "Faded Lines", artist: "Mora Vale", plays: 6, art: "", id: 7 }] },
            tracks: { count: 0, change: null, top: [] },
            hours: Array.from({ length: 24 }, (_, h) => h === 22 ? 9 : 1),
            weekdays: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].map((label, i) => ({ label: label, plays: i + 1 })),
            genres: [], decades: [{ decade: "1990", plays: 3 }],
            facts: { per_day: 1.5, best_day: { label: "Sat 4 Oct", plays: 6 }, bit_perfect: 3, events: 4, skips: 1 }
        })
    }
    function mosaic(anchor, back, firstDay, rev) {
        asked.push("mosaic:" + anchor + ":" + back)
        const year = 2026 - back
        return JSON.stringify({
            year: year, earlier: back < 1, later: back > 0, columns: 2, levels: 4, most: 8, plays: 10 + back,
            rows: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            months: [{ label: "Jan", column: 0 }],
            cells: [
                { date: year + "-01-01", column: 0, row: 3, plays: 2, level: 1, later: false, tip: "Thu 1 Jan " + year },
                { date: year + "-01-05", column: 1, row: 0, plays: 8, level: 4, later: false, tip: "Mon 5 Jan " + year },
                { date: year + "-01-06", column: 1, row: 1, plays: 0, level: 0, later: true, tip: "Tue 6 Jan " + year }
            ]
        })
    }
    // 2024 is a year with nothing in it; a year of 0 is last month.
    function recap(year, month, rev) {
        asked.push("recap:" + year + ":" + month)
        const at = year === 0 ? { year: 2026, month: 9 } : { year: year, month: month }
        const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        const names = (kind) => Array.from({ length: 5 }, (_, i) => ({ name: kind + " " + (i + 1), title: kind + " " + (i + 1), art: "", id: kind === "Album" ? i + 7 : 0, plays: 50 - i * 10, minutes: 200 - i * 40 }))
        return JSON.stringify({
            year: at.year, month: at.month, title: "Your " + (at.month ? months[at.month - 1] + " " : "") + at.year + " recap",
            first_year: 2025, first_month: 3, now_year: 2026, now_month: 10, months: months, any: true,
            empty: at.year === 2024,
            artists: names("Artist"), albums: names("Album"), tracks: names("Track"),
            minutes: 3030, plays: 868, per_day: 29.9, tracks_count: 345, albums_count: 205, artists_count: 145,
            new_tracks: 63, new_albums: 24, new_artists: 13
        })
    }
    function history(page, perPage, query, rev) {
        const all = query === "" ? total : 3
        const pages = Math.max(1, Math.ceil(all / perPage))
        const at = Math.min(page, pages)
        const count = Math.max(0, Math.min(perPage, all - (at - 1) * perPage))
        const rows = Array.from({ length: count }, (_, i) => ({ day: i < 2 ? "Today" : "Yesterday", clock: "08:0" + (i % 10), title: (query === "" ? "Track " : "Match ") + ((at - 1) * perPage + i + 1), artist: "Mora Vale", album: "Faded Lines", played: "0:31", length: "4:00", quality: "16/44.1", bit_perfect: i === 0, skipped: i === 1, art: "", album_id: 7 }))
        return JSON.stringify({ page: at, pages: pages, per_page: perPage, total: all, rows: rows })
    }
    function exportHistory() {
        exports.push("csv")
        stats.saved(total + " listens saved")
    }
    function picturePath() {
        return ""
    }
    function savePicture(from, name) {
        exports.push("png:" + name)
    }
}
