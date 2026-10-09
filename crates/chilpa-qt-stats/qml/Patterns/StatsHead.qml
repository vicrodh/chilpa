import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Theme"

// The stats screen's head: its title and what the period is compared
// with, the history and the export; under them the kind of period, and the
// arrows and the calendar that walk through them, with a Today when the
// period shown is not today's.
Column {
    id: head

    property var report: ({})
    property string kind
    // What the last export said, for a moment.
    property string note

    signal kindChosen(string kind)
    signal dayChosen(string date)
    signal historyAsked()
    signal tableAsked()
    signal pictureAsked()

    readonly property var kinds: ["day", "week", "month", "year", "all"]

    spacing: Metrics.gap12

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
            objectName: "statsComparison"
            anchors.left: title.right
            anchors.leftMargin: Metrics.gap12
            anchors.right: tools.left
            anchors.rightMargin: Metrics.gap12
            anchors.baseline: title.baseline
            text: (head.kind === "all" ? head.report.range : head.report.comparison) || ""
            elide: Text.ElideRight
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Row {
            id: tools
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8

            Text {
                objectName: "statsNote"
                anchors.verticalCenter: parent.verticalCenter
                visible: head.note !== ""
                text: head.note
                font.pointSize: Type.small
                color: Theme.color.statusPositive
            }
            PillButton {
                objectName: "openHistory"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.history
                text: I18n.tr("Listening history", I18n.rev)
                onClicked: head.historyAsked()
            }
            PillButton {
                id: exporter
                objectName: "statsExport"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.download
                text: I18n.tr("Export", I18n.rev)
                onClicked: exports.open()

                Menu {
                    id: exports
                    y: exporter.height + Metrics.gap4
                    x: exporter.width - width
                    entries: [
                        { text: I18n.tr("Listening history (CSV)", I18n.rev) },
                        { text: I18n.tr("This view as an image (PNG)", I18n.rev) }
                    ]
                    onChosen: (index) => index === 0 ? head.tableAsked() : head.pictureAsked()
                }
            }
        }
    }
    Item {
        width: parent.width
        height: Metrics.button28

        SegmentedControl {
            objectName: "statsPeriod"
            anchors.verticalCenter: parent.verticalCenter
            options: [
                { text: head.report.today === false ? I18n.tr("Day", I18n.rev) : I18n.tr("Today", I18n.rev) },
                { text: I18n.tr("Week", I18n.rev) },
                { text: I18n.tr("Month", I18n.rev) },
                { text: I18n.tr("Year", I18n.rev) },
                { text: I18n.tr("All time", I18n.rev) }
            ]
            current: Math.max(0, head.kinds.indexOf(head.kind))
            onActivated: (index) => head.kindChosen(head.kinds[index])
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap4
            visible: head.kind !== "all"

            IconButton {
                objectName: "statsEarlier"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.chevronLeft
                tip: I18n.tr("Earlier", I18n.rev)
                onClicked: head.dayChosen(head.report.earlier || "")
            }
            PillButton {
                id: day
                objectName: "statsDay"
                anchors.verticalCenter: parent.verticalCenter
                kind: "ghost"
                glyph: Icons.calendar
                text: head.report.range || ""
                onClicked: calendar.open()

                DatePicker {
                    id: calendar
                    objectName: "statsCalendar"
                    y: day.height + Metrics.gap4
                    x: day.width - width
                    selected: head.report.anchor || ""
                    latest: head.report.now || ""
                    onPicked: (date) => head.dayChosen(date)
                }
            }
            IconButton {
                objectName: "statsLater"
                anchors.verticalCenter: parent.verticalCenter
                enabled: (head.report.later || "") !== ""
                glyph: Icons.chevronRight
                tip: I18n.tr("Later", I18n.rev)
                onClicked: head.dayChosen(head.report.later)
            }
            PillButton {
                objectName: "statsToday"
                anchors.verticalCenter: parent.verticalCenter
                visible: head.report.current === false
                text: I18n.tr("Today", I18n.rev)
                onClicked: head.dayChosen("")
            }
        }
    }
}
