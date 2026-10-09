import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"

// When and what: the listening clock, the days of the week, the genres and
// the decades, two cards across while they fit.
Flow {
    id: charts

    property var report: ({})
    readonly property int across: Math.max(1, Math.min(2, Math.floor((width + spacing) / (Charts.chartMin + spacing))))
    readonly property real card: (width - (across - 1) * spacing) / across
    readonly property var days: [I18n.tr("Mon", I18n.rev), I18n.tr("Tue", I18n.rev), I18n.tr("Wed", I18n.rev), I18n.tr("Thu", I18n.rev), I18n.tr("Fri", I18n.rev), I18n.tr("Sat", I18n.rev), I18n.tr("Sun", I18n.rev)]

    spacing: Metrics.gap16

    StatsCard {
        width: charts.card
        height: Charts.chartTall
        title: I18n.tr("Listening clock", I18n.rev)
        note: clock.peak < 0 ? "" : I18n.trArgs("Most at {}:00", [String(clock.peak).padStart(2, "0")], I18n.rev)

        ListeningClock {
            id: clock
            anchors.fill: parent
            hours: charts.report.hours || []
        }
    }
    StatsCard {
        width: charts.card
        height: Charts.chartTall
        title: I18n.tr("Days of the week", I18n.rev)

        ColumnBars {
            anchors.fill: parent
            points: (charts.report.weekdays || []).map((plays, day) => ({ label: charts.days[day], value: plays }))
        }
    }
    StatsCard {
        width: charts.card
        height: Charts.chartTall
        title: I18n.tr("Genres", I18n.rev)

        RankBars {
            width: parent.width
            points: (charts.report.genres || []).map(genre => ({ label: genre.name, value: genre.plays }))
        }
        Text {
            anchors.centerIn: parent
            visible: (charts.report.genres || []).length === 0
            text: I18n.tr("No genres in this period.", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textTertiary
        }
    }
    StatsCard {
        width: charts.card
        height: Charts.chartTall
        title: I18n.tr("Decades", I18n.rev)

        ColumnBars {
            anchors.fill: parent
            tint: Theme.color.statsAlbums
            points: (charts.report.decades || []).map(decade => ({ label: I18n.trArgs("{}s", [decade.decade], I18n.rev), value: decade.plays }))
        }
    }
}
