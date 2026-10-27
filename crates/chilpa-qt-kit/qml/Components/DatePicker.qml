import QtQuick
import QtQuick.Controls.Basic as Controls
import QtQuick.Effects
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// A month of days to pick one from: its name with arrows to the months
// around it, the days of the week as the language starts them, and a
// Today at the foot. Days after `latest` cannot be picked. Dates go in
// and out as "2026-10-09".
Controls.Popup {
    id: picker
    PopupInput { surface: picker }

    // The day marked as chosen, and the last one that can be picked.
    property string selected
    property string latest
    // The month on show.
    property int year: new Date().getFullYear()
    property int month: new Date().getMonth()

    signal picked(string date)

    function iso(date) {
        return Qt.formatDate(date, "yyyy-MM-dd")
    }
    function show(date) {
        const day = date ? new Date(date + "T00:00") : new Date()
        year = day.getFullYear()
        month = day.getMonth()
    }
    function step(by) {
        const next = new Date(year, month + by, 1)
        year = next.getFullYear()
        month = next.getMonth()
    }

    modal: true
    dim: false
    closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnReleaseOutside
    // So Escape reaches it and closes it.
    focus: true
    padding: Metrics.gap12
    onAboutToShow: show(selected)

    background: Item {
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusMenu
            color: Theme.color.surfaceRaised
            border.width: 1
            border.color: Theme.color.strokeHairline
        }
    }

    contentItem: Column {
        spacing: Metrics.gap8

        Item {
            width: grid.width
            height: Metrics.button28

            IconButton {
                objectName: "pickerEarlier"
                anchors.left: parent.left
                glyph: Icons.chevronLeft
                tip: I18n.tr("Previous month", I18n.rev)
                onClicked: picker.step(-1)
            }
            Text {
                anchors.centerIn: parent
                text: grid.title
                font.pointSize: Type.body
                font.weight: Type.semibold
                color: Theme.color.textPrimary
            }
            IconButton {
                objectName: "pickerLater"
                anchors.right: parent.right
                enabled: picker.iso(new Date(picker.year, picker.month + 1, 1)) <= picker.latest
                glyph: Icons.chevronRight
                tip: I18n.tr("Next month", I18n.rev)
                onClicked: picker.step(1)
            }
        }
        Controls.DayOfWeekRow {
            width: grid.width
            locale: grid.locale
            delegate: Text {
                required property string shortName
                text: shortName
                horizontalAlignment: Text.AlignHCenter
                font.pointSize: Type.label
                color: Theme.color.textTertiary
            }
        }
        Controls.MonthGrid {
            id: grid
            objectName: "pickerDays"
            width: Metrics.datePickerWide
            // The grid shares its own height among the six weeks it always
            // shows; its days do not size it.
            height: Metrics.datePickerDay * 6 // derived: six weeks
            month: picker.month
            year: picker.year
            locale: Qt.locale()
            delegate: Rectangle {
                id: cell
                required property var model
                readonly property string day: picker.iso(model.date)
                readonly property bool open: day <= picker.latest
                readonly property bool chosen: day === picker.selected
                implicitHeight: Metrics.datePickerDay
                radius: Metrics.radiusItem
                color: chosen ? Theme.color.accent : hover.hovered && open ? Theme.color.stateHover : "transparent"
                opacity: model.month === picker.month ? 1 : Metrics.disabledOpacity
                Text {
                    anchors.centerIn: parent
                    text: cell.model.day
                    font.pointSize: Type.small
                    font.weight: cell.model.today ? Type.bold : Type.regular
                    font.features: { "tnum": 1 }
                    color: cell.chosen ? Theme.color.textOnAccent : cell.open ? Theme.color.textPrimary : Theme.color.textTertiary
                }
                HoverHandler {
                    id: hover
                    cursorShape: cell.open ? Qt.PointingHandCursor : Qt.ArrowCursor
                }
            }
            onClicked: (date) => {
                const day = picker.iso(date)
                if (day > picker.latest)
                    return
                picker.close()
                picker.picked(day)
            }
        }
        PillButton {
            objectName: "pickerToday"
            anchors.horizontalCenter: parent.horizontalCenter
            kind: "ghost"
            text: I18n.tr("Today", I18n.rev)
            onClicked: {
                picker.close()
                picker.picked(picker.latest)
            }
        }
    }
}
