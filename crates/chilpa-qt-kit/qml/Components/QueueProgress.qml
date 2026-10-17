import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "../Theme"

// I keep navigation and playback usable while a long list is being added.
Rectangle {
    id: notice
    objectName: "queueProgress"
    width: contents.implicitWidth + Metrics.gap12 * 2 // derived: padding on both sides
    height: Metrics.button32
    radius: Metrics.radiusMenu
    color: Theme.color.surfaceRaised
    border.color: Theme.color.strokeHairline
    visible: QueueActivity.busy
    Timer { interval: 100; running: QueueActivity.busy; repeat: true; onTriggered: QueueActivity.refresh() }
    Row {
        id: contents
        anchors.centerIn: parent
        spacing: Metrics.gap8
        BusyIndicator {
            width: Metrics.button24; height: width
            running: notice.visible
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Adding to queue…", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textPrimary
        }
    }
}
