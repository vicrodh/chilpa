import QtQuick
import "../Theme"

// The ring that says a control has the keyboard, as QBZ draws it: 2 px on
// the control's own edge, in the accent, or in the color of the glyphs over
// a control filled with the accent; `gap` sets it outside, as on a slider.
Rectangle {
    property Item control: parent
    property bool onAccent: false
    property real gap: 0
    readonly property real edge: control && control.radius !== undefined ? control.radius : Metrics.radiusControl
    anchors.fill: control
    anchors.margins: -gap
    radius: edge + gap
    color: "transparent"
    border.width: Metrics.focusRing
    border.color: onAccent ? Theme.color.textOnAccent : Theme.color.accent
    visible: !!control && control.activeFocus
}
