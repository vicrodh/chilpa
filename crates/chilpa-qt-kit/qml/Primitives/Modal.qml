import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import "../Theme"

// One visual and spacing contract for every application modal.
Popup {
    id: root

    default property alias body: inset.data

    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    contentItem: Item {
        implicitWidth: inset.implicitWidth + Metrics.modalInsets
        implicitHeight: inset.implicitHeight + Metrics.modalInsets

        // A press in the modal outside its field takes the keyboard from it.
        FocusRelease {}

        Item {
            id: inset
            x: Metrics.modalInset
            y: Metrics.modalInset
            width: Math.max(0, parent.width - Metrics.modalInsets)
            height: Math.max(0, parent.height - Metrics.modalInsets)
            implicitWidth: childrenRect.width
            implicitHeight: childrenRect.height
        }
    }

    background: Item {
        Hold {}
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
