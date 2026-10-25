import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// The footer's volume (02-mini §1.5, QBZ's MiniVolume): one button, and a
// pill that opens to its RIGHT, over the transport, with mute, the slider
// and the number. A click anywhere else closes it: the layer that takes
// that click is a plain area far larger than the card and cut by the
// card's clip, under the pill and over the button, which is why a second
// click on the button closes rather than reopens. The pill's mute button
// always shows the muted glyph, as QBZ's does: it is a button, not an
// indicator. A locked volume leaves mute and the slider unusable.
Rectangle {
    id: control

    property int side: Metrics.button30
    property int iconSize: Metrics.iconNav
    property bool lightForeground: false
    property bool popupOpen: false
    readonly property bool locked: Player.volumeLocked

    width: side
    height: side
    color: "transparent"

    TransportButton {
        glyph: Player.muted || Player.volume === 0 ? Icons.volume3 : Icons.volume
        iconSize: control.iconSize
        side: control.side
        lightForeground: control.lightForeground
        onClicked: control.popupOpen = true
    }

    // no cursor: the layer that closes the pill, not a control
    MouseArea {
        z: 1
        visible: control.popupOpen
        enabled: control.popupOpen
        x: -Card.dismissReach / 2 // derived: centered on the button
        y: -Card.dismissReach / 2 // derived: centered on the button
        width: Card.dismissReach
        height: Card.dismissReach
        onClicked: control.popupOpen = false
    }

    Rectangle {
        id: pill
        z: 2
        visible: control.popupOpen
        x: control.side + Card.pillApart
        y: Math.round((control.height - height) / 2) // derived: centered on the button
        width: Card.pillLong
        height: Card.pillTall
        radius: height / 2 // derived: a pill
        antialiasing: true
        color: Theme.color.surfacePanel
        border.width: Card.edge
        border.color: Theme.color.miniCapsuleHover

        // no cursor: swallows the pill's own clicks so they do not close it
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onClicked: {}
        }

        Row {
            x: Card.volumeInnerPad
            anchors.verticalCenter: parent.verticalCenter
            spacing: Card.pillBetween

            TransportButton {
                objectName: "miniVolumeMute"
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.volume3
                iconSize: Card.volumeMuteIcon
                side: Card.volumeMuteButton
                able: !control.locked
                onClicked: Player.chooseMuted(!Player.muted)
            }
            LevelSlider {
                objectName: "miniVolumeSlider"
                anchors.verticalCenter: parent.verticalCenter
                width: Card.volumeSlider
                enabled: !control.locked
                value: Player.volume
                onReleased: Player.persistVolume()
                onMoved: (level) => Player.chooseVolume(level)
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Card.volumeNumber
                text: Player.volume
                color: Theme.color.textTertiary
                font.pointSize: Card.pillNumberText
                horizontalAlignment: Text.AlignHCenter
            }
        }
    }
}
