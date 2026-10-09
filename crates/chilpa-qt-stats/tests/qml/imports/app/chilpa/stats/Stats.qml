pragma Singleton
import QtQuick

// The stats bridge as the tests see it: the periods and pages it was asked
// for, and documents made from them.
QtObject {
    // What was asked, kept where a binding that calls `report` does not
    // depend on it.
    readonly property var asked: []
    property int total: 130
    function report(period, back, rev) {
        asked.push(period + ":" + back)
        const days = period === "week" ? 7 : 3
        return JSON.stringify({
            period: period, back: back, range: period + " " + back, plays: 10 + back, plays_change: back > 0 ? -10 : 25,
            listened: "1 h 2 min", listened_change: null,
            series: Array.from({ length: days }, (_, i) => ({ label: "d" + i, tip: "day " + i, plays: i, prev: days - i })),
            artists: { count: 2, change: 100, top: [{ name: "Mora Vale", plays: 6, art: "" }, { name: "Yoko Kanno", plays: 3, art: "" }] },
            albums: { count: 1, change: null, top: [{ title: "Faded Lines", artist: "Mora Vale", plays: 6, art: "", id: 7 }] },
            tracks: { count: 0, change: null, top: [] },
            hours: Array.from({ length: 24 }, (_, h) => h === 22 ? 9 : 1), weekdays: [1, 2, 3, 4, 5, 6, 7],
            genres: [], decades: [{ decade: "1990", plays: 3 }],
            facts: { per_day: 1.5, best_day: { label: "Sat 4 Oct", plays: 6 }, bit_perfect: 3, events: 4, skips: 1 }
        })
    }
    function history(page, perPage, rev) {
        const pages = Math.max(1, Math.ceil(total / perPage))
        const at = Math.min(page, pages)
        const count = Math.max(0, Math.min(perPage, total - (at - 1) * perPage))
        const rows = Array.from({ length: count }, (_, i) => ({ day: i < 2 ? "Today" : "Yesterday", clock: "08:0" + (i % 10), title: "Track " + ((at - 1) * perPage + i + 1), artist: "Mora Vale", album: "Faded Lines", played: "3:00", length: "4:00", quality: "16/44.1", bit_perfect: i === 0, skipped: i === 1, art: "", album_id: 7 }))
        return JSON.stringify({ page: at, pages: pages, per_page: perPage, total: total, rows: rows })
    }
}
