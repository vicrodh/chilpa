import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"

// One visual and spacing contract for every application modal.
Popup {
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: Metrics.gap20
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Item {
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowElevatedY
            blur: Metrics.shadowElevatedBlur
            color: Theme.color.shadowElevated
        }
        RectangularShadow {
            anchors.fill: panel
            radius: panel.radius
            offset.y: Metrics.shadowKeyY
            blur: Metrics.shadowKeyBlur
            color: Theme.color.shadowElevatedKey
        }
        Rectangle {
            id: panel
            anchors.fill: parent
            radius: Metrics.radiusPanel
            color: Theme.color.surfaceRaised
            Edge {}
        }
    }
}
