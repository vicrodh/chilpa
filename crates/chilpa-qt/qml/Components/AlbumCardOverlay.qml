import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// What a card shows under the pointer: the scrim and its play button. Only
// one exists at a time, so it can afford what a card cannot.
Rectangle {
    id: overlay

    signal play()

    radius: Metrics.radiusCard
    color: Theme.color.overlayScrim

    Rectangle {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: Metrics.overlayPad
        width: Metrics.overlayPlay
        height: Metrics.overlayPlay
        radius: width / 2 // derived: a circle
        color: button.containsMouse ? Theme.color.accentHover : Theme.color.accent

        Icon {
            anchors.centerIn: parent
            glyph: Icons.playerPlayFilled
            filled: true
            size: Metrics.iconNav
            color: Theme.color.textOnAccent
        }

        MouseArea {
            id: button
            anchors.fill: parent
            hoverEnabled: true
            onClicked: overlay.play()
            onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Play", I18n.rev), parent) : TipState.hide(parent)
        }
    }
}
