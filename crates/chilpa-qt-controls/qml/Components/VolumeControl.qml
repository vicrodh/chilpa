import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One compact speaker in the player bar opens the complete volume control
// upward. A direct route with no knob to move stays locked and explains why.
Item {
    id: control

    // Open under the button instead, for a bar at the window's top.
    property bool opensDown: false

    // How long a keyboard volume change stays on screen after the last one.
    property int peekMs: 2000

    implicitWidth: Metrics.button28
    implicitHeight: Metrics.button28

    // Shows the level for a moment without taking focus or the clicks. Each
    // call restarts the wait, so a held key keeps it up. Does nothing while
    // the full control is open, or when the control is not there to point at.
    function peek() {
        if (!visible || Player.volumeLocked || volume.opened)
            return
        glance.place()
        glance.visible = true
        glanceTimer.restart()
    }

    IconButton {
        id: opener
        anchors.fill: parent
        opacity: Player.volumeLocked ? Metrics.disabledOpacity : 1
        glyph: Player.muted || Player.volume === 0 ? Icons.volume3 : Player.volume < 50 ? Icons.volume2 : Icons.volume
        iconSize: Metrics.iconNav
        tip: Player.volumeLocked ? Player.volumeNote
            : I18n.trArgs("Volume {}%", [String(Player.muted ? 0 : Player.volume)], I18n.rev)
        onClicked: {
            if (!Player.volumeLocked)
                volume.opened ? volume.close() : volume.open()
        }
    }

    Component {
        id: panelFrame
        Item {
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
                radius: Metrics.radiusMenu
                color: Theme.color.surfaceRaised

                Edge {}
            }
        }
    }

    Popup {
        id: volume
        PopupInput { surface: volume }

        // A click outside closes it and is nothing else: it is not also a click
        // on what is there.
        modal: true
        dim: false

        x: (control.width - width) / 2 // derived: centered over its button
        y: control.opensDown ? control.height + Metrics.gap8 : -height - Metrics.gap8
        width: Metrics.volumePopupWidth
        padding: Metrics.gap8
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside

        background: Loader { sourceComponent: panelFrame }

        contentItem: Column {
            spacing: Metrics.gap6
            Text {
                width: parent.width
                visible: Cast.active
                text: I18n.tr("Receiver volume", I18n.rev)
                wrapMode: Text.WordWrap
                horizontalAlignment: Text.AlignHCenter
                font.pointSize: Type.micro
                color: Theme.color.textSecondary
            }

            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: String(Player.muted ? 0 : Player.volume) + "%"
                font.pointSize: Type.micro
                font.features: { "tnum": 1 }
                color: Theme.color.textSecondary
            }
            LevelSlider {
                objectName: "volumeLevel"
                anchors.horizontalCenter: parent.horizontalCenter
                vertical: true
                enabled: !Player.volumeLocked
                value: Player.muted ? 0 : Player.volume
                tip: I18n.trArgs("Volume {}%", [String(Player.volume)], I18n.rev)
                onReleased: Player.persistVolume()
                onMoved: (level) => {
                    // Moving it is wanting sound: a muted volume comes back.
                    if (Player.muted)
                        Player.chooseMuted(false)
                    Player.chooseVolume(level)
                }
            }
            IconButton {
                anchors.horizontalCenter: parent.horizontalCenter
                size: Metrics.button24
                glyph: Player.muted ? Icons.volume3 : Icons.volume
                tip: Player.muted ? I18n.tr("Unmute", I18n.rev) : I18n.tr("Mute", I18n.rev)
                checked: Player.muted
                onClicked: Player.chooseMuted(!Player.muted)
            }
        }
    }

    Timer {
        id: glanceTimer
        interval: control.peekMs
        onTriggered: glance.visible = false
    }

    // The same panel as the control, read-only. It is not a Popup: a popup
    // holds the window's shortcuts while it shows, and a held volume key has
    // to keep arriving. It sits on the window's content, over the player bar.
    Item {
        id: glance
        objectName: "volumeGlance"
        parent: control.Window.window ? control.Window.window.contentItem : null
        visible: false
        z: Metrics.tooltipLayer
        width: Metrics.volumePopupWidth
        height: glanceColumn.implicitHeight + 2 * Metrics.gap8 // derived: the content and its padding

        Loader { anchors.fill: parent; sourceComponent: panelFrame }

        Column {
            id: glanceColumn
            x: Metrics.gap8
            y: Metrics.gap8
            width: parent.width - 2 * Metrics.gap8 // derived: inside the padding
            spacing: Metrics.gap6
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: String(Player.muted ? 0 : Player.volume) + "%"
                font.pointSize: Type.micro
                font.features: { "tnum": 1 }
                color: Theme.color.textSecondary
            }
            LevelSlider {
                anchors.horizontalCenter: parent.horizontalCenter
                vertical: true
                enabled: false
                value: Player.muted ? 0 : Player.volume
            }
        }

        // Over the button, above or below as the full control opens.
        function place() {
            const at = control.mapToItem(glance.parent, (control.width - width) / 2, control.opensDown ? control.height + Metrics.gap8 : -height - Metrics.gap8) // derived: as the popup
            x = at.x
            y = at.y
        }
    }

    Connections {
        target: volume
        function onOpenedChanged() {
            if (volume.opened) {
                glanceTimer.stop()
                glance.visible = false
            }
        }
    }
}
