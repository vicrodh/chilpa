import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Patterns"

// The recap: a month or a year gone by as one card -- its first artist,
// record and track, and nine numbers. It opens on the month before this
// one; beside the card, the months and the years to choose from, and over
// it whether each name also says the time it sounded.
Flickable {
    id: page

    // The period asked for; a year of 0 is the month before this one.
    property int year: 0
    property int month: 0
    property string mode: "month"
    // The year whose months are offered; 0 follows the recap shown.
    property int shownYear: 0
    property bool timed: SettingsModel.kept("ui.recap.timed") === "true"
    readonly property var recap: JSON.parse(Stats.recap(year, month, I18n.rev + Library.contentRev + Library.remoteRev + Library.remoteArtRev) || "{}")
    readonly property real wide: Math.min(width - 2 * Metrics.contentX, Charts.recapWidest) // derived: padding on both sides
    // Too narrow for the periods beside the card: they go above it.
    readonly property bool stacked: wide < Charts.recapStacks

    function pick(nextYear, nextMonth) {
        year = nextYear
        month = nextMonth
        shownYear = 0
        contentY = 0
    }
    function time(on) {
        timed = on
        SettingsModel.keep("ui.recap.timed", on ? "true" : "false")
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
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Recap", I18n.rev)
                font.pointSize: Type.title
                font.weight: Type.bold
                font.letterSpacing: Type.titleTracking
                color: Theme.color.textPrimary
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap8

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Show time listened", I18n.rev)
                    font.pointSize: Type.small
                    color: Theme.color.textSecondary
                }
                Toggle {
                    objectName: "recapTimed"
                    anchors.verticalCenter: parent.verticalCenter
                    compact: true
                    checked: page.timed
                    onToggled: (on) => page.time(on)
                }
            }
        }
        Item {
            width: parent.width
            height: page.stacked ? periods.height + Metrics.gap16 + card.height : Math.max(periods.height, card.height)

            RecapPeriods {
                id: periods
                objectName: "recapPeriods"
                width: page.stacked ? parent.width : Charts.recapPeriods
                recap: page.recap
                mode: page.mode
                shownYear: page.shownYear > 0 ? page.shownYear : (page.recap.year || 0)
                onModeChosen: (mode) => page.mode = mode
                onYearShown: (year) => page.shownYear = year
                onPicked: (year, month) => page.pick(year, month)
            }
            RecapCard {
                id: card
                objectName: "recapCard"
                x: page.stacked ? 0 : periods.width + Metrics.gap24
                y: page.stacked ? periods.height + Metrics.gap16 : 0
                width: parent.width - x
                recap: page.recap
                timed: page.timed
            }
        }
    }
}
