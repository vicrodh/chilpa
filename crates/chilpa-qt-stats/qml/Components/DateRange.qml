import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A span of days to look at: its first and its last day, each picked from
// a month and either left open, and a way back to every day. A first day
// after the last, or a last before the first, takes the other end with it.
// Dates go in and out as "2026-10-09"; none after `latest` can be picked.
Row {
    id: range

    property string from
    property string to
    property string latest

    signal chosen(string from, string to)

    function label(date, open) {
        return date === "" ? open : Qt.formatDate(new Date(date + "T00:00"), Locale.ShortFormat)
    }

    spacing: Metrics.gap6

    PillButton {
        id: start
        objectName: "rangeFrom"
        anchors.verticalCenter: parent.verticalCenter
        kind: "ghost"
        glyph: Icons.calendar
        text: range.label(range.from, I18n.tr("First day", I18n.rev))
        onClicked: first.open()

        DatePicker {
            id: first
            objectName: "rangeFromPicker"
            y: start.height + Metrics.gap4
            selected: range.from
            latest: range.latest
            onPicked: (date) => range.chosen(date, range.to !== "" && range.to < date ? date : range.to)
        }
    }
    Icon {
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.arrowRight
        size: Metrics.iconSm
        color: Theme.color.textTertiary
    }
    PillButton {
        id: end
        objectName: "rangeTo"
        anchors.verticalCenter: parent.verticalCenter
        kind: "ghost"
        text: range.label(range.to, I18n.tr("Last day", I18n.rev))
        onClicked: last.open()

        DatePicker {
            id: last
            objectName: "rangeToPicker"
            y: end.height + Metrics.gap4
            selected: range.to
            latest: range.latest
            onPicked: (date) => range.chosen(range.from !== "" && range.from > date ? date : range.from, date)
        }
    }
    IconButton {
        objectName: "rangeClear"
        anchors.verticalCenter: parent.verticalCenter
        visible: range.from !== "" || range.to !== ""
        glyph: Icons.x
        size: Metrics.button26
        tip: I18n.tr("All days", I18n.rev)
        onClicked: range.chosen("", "")
    }
}
