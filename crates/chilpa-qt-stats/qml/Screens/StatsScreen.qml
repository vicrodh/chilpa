import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Patterns"

// Listening stats (borradores/listening-stats-v1-2026-10-09), with
// Last.fm's weekly report as the model: a week, a month, a year or all of
// it, the arrows walking to the periods before; the period against the
// one before it, its artists, records and tracks, when and what it was,
// and a few facts. The history, listen by listen, is a page of its own.
Flickable {
    id: page

    readonly property var periods: ["week", "month", "year", "all"]
    property string period: periods.indexOf(SettingsModel.kept("ui.stats.period")) >= 0 ? SettingsModel.kept("ui.stats.period") : "week"
    property int back: 0
    readonly property var report: JSON.parse(Stats.report(period, back, I18n.rev) || "{}")
    readonly property real wide: Math.min(width - 2 * Metrics.contentX, Charts.widest) // derived: padding on both sides

    function choose(index) {
        period = periods[index]
        back = 0
        SettingsModel.keep("ui.stats.period", period)
    }

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    Column {
        id: column
        x: (page.width - page.wide) / 2 // derived: centered past its widest
        y: Metrics.contentTop
        width: page.wide
        spacing: Metrics.gap16

        Item {
            width: parent.width
            height: Metrics.shelfHeader

            Text {
                id: title
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Listening stats", I18n.rev)
                font.pointSize: Type.title
                font.weight: Type.bold
                font.letterSpacing: Type.titleTracking
                color: Theme.color.textPrimary
            }
            Text {
                objectName: "statsRange"
                anchors.left: title.right
                anchors.leftMargin: Metrics.gap12
                anchors.right: tools.left
                anchors.rightMargin: Metrics.gap12
                anchors.baseline: title.baseline
                text: page.report.range || ""
                elide: Text.ElideRight
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
            Row {
                id: tools
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap8

                IconButton {
                    objectName: "statsEarlier"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: page.period !== "all"
                    glyph: Icons.chevronLeft
                    tip: I18n.tr("Earlier", I18n.rev)
                    onClicked: page.back += 1
                }
                IconButton {
                    objectName: "statsLater"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: page.period !== "all"
                    enabled: page.back > 0
                    glyph: Icons.chevronRight
                    tip: I18n.tr("Later", I18n.rev)
                    onClicked: page.back -= 1
                }
                SegmentedControl {
                    objectName: "statsPeriod"
                    anchors.verticalCenter: parent.verticalCenter
                    options: [{ text: I18n.tr("Week", I18n.rev) }, { text: I18n.tr("Month", I18n.rev) }, { text: I18n.tr("Year", I18n.rev) }, { text: I18n.tr("All time", I18n.rev) }]
                    current: page.periods.indexOf(page.period)
                    onActivated: (index) => page.choose(index)
                }
            }
        }
        StatsHero {
            width: parent.width
            report: page.report
        }
        StatsTops {
            width: parent.width
            report: page.report
        }
        StatsCharts {
            width: parent.width
            report: page.report
        }
        StatsFacts {
            width: parent.width
            report: page.report
        }
        PillButton {
            objectName: "openHistory"
            glyph: Icons.history
            text: I18n.tr("See your listening history", I18n.rev)
            onClicked: Navigation.go("listeningHistory", 0)
        }
    }
}
