import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Which recap: of a month or of a year. By month, a year to step through
// and its twelve months; by year, every year since the first listen. What
// has not happened yet, and what came before anything was played, cannot
// be chosen. Stepping the year only changes which months are offered: the
// recap changes when a month is picked.
Column {
    id: periods

    // The recap shown: its year and month say which button is the chosen
    // one, and its first and current months what can be chosen.
    property var recap: ({})
    // "month" or "year".
    property string mode: "month"
    // The year whose months are offered.
    property int shownYear: 0

    signal modeChosen(string mode)
    signal yearShown(int year)
    signal picked(int year, int month)

    readonly property var modes: ["month", "year"]
    readonly property int firstYear: recap.first_year || 0
    readonly property int nowYear: recap.now_year || 0
    readonly property real cell: (width - Metrics.gap8) / 2 // derived: two across

    function offered(year, month) {
        const at = year * 12 + month // derived: months since year zero
        return at >= firstYear * 12 + (recap.first_month || 1) && at <= nowYear * 12 + (recap.now_month || 12) // derived: months since year zero
    }

    spacing: Metrics.gap12

    SegmentedControl {
        objectName: "recapMode"
        options: [{ text: I18n.tr("Monthly", I18n.rev) }, { text: I18n.tr("Yearly", I18n.rev) }]
        current: Math.max(0, periods.modes.indexOf(periods.mode))
        onActivated: (index) => periods.modeChosen(periods.modes[index])
    }
    Item {
        visible: periods.mode === "month"
        width: parent.width
        height: Metrics.button28

        IconButton {
            objectName: "recapYearEarlier"
            anchors.verticalCenter: parent.verticalCenter
            enabled: periods.shownYear > periods.firstYear
            glyph: Icons.chevronLeft
            tip: I18n.tr("Earlier year", I18n.rev)
            onClicked: periods.yearShown(periods.shownYear - 1)
        }
        Text {
            objectName: "recapYear"
            anchors.centerIn: parent
            text: periods.shownYear > 0 ? String(periods.shownYear) : ""
            font.pointSize: Type.bodyLarge
            font.weight: Type.semibold
            font.features: { "tnum": 1 }
            color: Theme.color.textPrimary
        }
        IconButton {
            objectName: "recapYearLater"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            enabled: periods.shownYear < periods.nowYear
            glyph: Icons.chevronRight
            tip: I18n.tr("Later year", I18n.rev)
            onClicked: periods.yearShown(periods.shownYear + 1)
        }
    }
    Flow {
        visible: periods.mode === "month"
        width: parent.width
        spacing: Metrics.gap8

        Repeater {
            model: periods.recap.months || []

            PillButton {
                required property var modelData
                required property int index
                objectName: "recapMonth" + (index + 1)
                width: periods.cell
                enabled: periods.offered(periods.shownYear, index + 1)
                opacity: enabled ? 1 : Metrics.disabledOpacity
                kind: periods.recap.month === index + 1 && periods.recap.year === periods.shownYear ? "primary" : "secondary"
                text: modelData
                onClicked: periods.picked(periods.shownYear, index + 1)
            }
        }
    }
    Flow {
        visible: periods.mode === "year"
        width: parent.width
        spacing: Metrics.gap8

        Repeater {
            model: periods.firstYear > 0 ? periods.nowYear - periods.firstYear + 1 : 0

            PillButton {
                required property int index
                readonly property int year: periods.firstYear + index
                objectName: "recapYear" + year
                width: periods.cell
                kind: periods.recap.month === 0 && periods.recap.year === year ? "primary" : "secondary"
                text: String(year)
                onClicked: periods.picked(year, 0)
            }
        }
    }
}
