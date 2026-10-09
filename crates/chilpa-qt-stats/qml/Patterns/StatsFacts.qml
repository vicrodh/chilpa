import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"

// A few facts of the period: plays a day, its busiest day, how much of it
// played bit-perfect and how much was skipped.
Flow {
    id: facts

    property var report: ({})
    readonly property var numbers: report.facts || ({})
    readonly property int across: Math.max(1, Math.min(4, Math.floor((width + spacing) / (Charts.factMin + spacing))))
    readonly property real card: (width - (across - 1) * spacing) / across

    spacing: Metrics.gap16

    FactCard {
        width: facts.card
        height: Charts.factTall
        glyph: Icons.chartBar
        value: Number(facts.numbers.per_day || 0).toLocaleString(Qt.locale(), "f", 1)
        label: I18n.tr("Plays a day", I18n.rev)
    }
    FactCard {
        width: facts.card
        height: Charts.factTall
        glyph: Icons.calendar
        value: facts.numbers.best_day ? facts.numbers.best_day.label : "—"
        label: facts.numbers.best_day ? I18n.trCount("Busiest day, {} play", "Busiest day, {} plays", facts.numbers.best_day.plays, I18n.rev) : I18n.tr("Busiest day", I18n.rev)
        tint: Theme.color.statsTracks
    }
    FactCard {
        width: facts.card
        height: Charts.factTall
        glyph: Icons.waveSquare
        value: (facts.numbers.events > 0 ? Math.round(facts.numbers.bit_perfect * 100 / facts.numbers.events) : 0) + "%"
        label: I18n.tr("Bit-perfect", I18n.rev)
        tint: Theme.color.statusPositive
    }
    FactCard {
        width: facts.card
        height: Charts.factTall
        glyph: Icons.playerTrackNext
        value: Number(facts.numbers.skips || 0).toLocaleString(Qt.locale(), "f", 0)
        label: I18n.tr("Skips", I18n.rev)
        tint: Theme.color.statsArtists
    }
}
