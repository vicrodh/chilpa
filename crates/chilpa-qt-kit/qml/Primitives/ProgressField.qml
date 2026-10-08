import QtQuick
import "../Theme"

// A background job's progress and the button that starts or cancels it
// (design/03 SettingsRow, progress). `progress` is a share from 0 to 1 as
// text, or empty while the job has none to show.
Row {
    id: field

    property string progress
    property string action

    signal asked()

    spacing: Metrics.gap12

    Rectangle {
        visible: field.progress !== ""
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.settingsNav
        height: Metrics.progress
        radius: height
        color: Theme.color.strokeHairline

        Rectangle {
            width: parent.width * Math.max(0, Math.min(1, Number(field.progress)))
            height: parent.height
            radius: parent.radius
            color: Theme.color.accent
        }
    }
    PillButton {
        anchors.verticalCenter: parent.verticalCenter
        text: field.action
        onClicked: field.asked()
    }
}
