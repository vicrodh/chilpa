pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The stats screens' numbers, with Last.fm's weekly report as the model
// (borradores/listening-stats-v1-2026-10-09).
QtObject {
    // Past this the page stops growing and stays centered.
    readonly property int widest: 1180
    // The hero: its numbers on the left, the bars of the period beside.
    readonly property int heroTall: 248
    readonly property int heroLead: 240
    readonly property real heroNumber: 48 * Type.pointsPerPixel
    readonly property real tileNumber: 30 * Type.pointsPerPixel
    readonly property real factNumber: 22 * Type.pointsPerPixel
    readonly property int barRound: 3
    readonly property int barApart: 2
    readonly property int barLabel: 20
    // The narrowest a bar's label may be before only some bars get one.
    readonly property int barLabelWide: 28
    // The tiles of artists, records and tracks; the first of each is
    // larger.
    readonly property int tileMin: 280
    readonly property int tileTall: 392
    readonly property int thumb: 36
    readonly property int thumbLead: 64
    readonly property int rankRow: 44
    readonly property int rankBar: 3
    // The chart cards: the listening clock and the bar charts.
    readonly property int chartMin: 320
    readonly property int chartTall: 280
    readonly property int clockHole: 30
    readonly property int clockLabel: 14
    readonly property int rankName: 108
    readonly property int rankCount: 40
    readonly property int rankBarTall: 10
    // A fact.
    readonly property int factMin: 200
    readonly property int factTall: 96
    // The history: a rich row, the day above its rows (lower over compact
    // rows), its fixed columns and the pager's buttons.
    readonly property int historyRow: 56
    readonly property int historyCover: 40
    readonly property int historyInset: Metrics.gap12
    readonly property int historySearch: 260
    readonly property int dayHeader: 44
    readonly property int dayHeaderCompact: 32
    readonly property int timeColumn: 56
    readonly property int skippedColumn: 96
    readonly property int qualityColumn: 80
    readonly property int perfectColumn: 36
    // A compact row's title takes at most this much of its column, and its
    // artist and record the rest.
    readonly property real historyTitleShare: 0.55
    readonly property int pagerButton: 32
    // The picture's signature: the logo beside its line, decoded at
    // four times its size so it stays sharp on a dense screen.
    readonly property int signatureLogo: 24
    readonly property int signatureDecode: 96
}
