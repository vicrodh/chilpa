// Previous, pause, next and stop, with their shortcuts.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import com.blitzfc.chilpa

// A Flow and not a Row, for the same reason as the actions: at a narrow
// window the buttons wrap instead of running off the edge.
Flow {
    id: root
    spacing: 8
    Layout.fillWidth: true
    Layout.preferredHeight: implicitHeight

    // With the cursor inside a text field, the space bar types a space;
    // it does not pause.
    readonly property bool typing:
        Window.window !== null && Window.window.activeFocusItem instanceof TextInput

    Shortcut {
        sequence: "Space"
        enabled: !root.typing && Chilpa.playing
        onActivated: Chilpa.pause(!Chilpa.paused)
    }
    Shortcut {
        sequence: "Ctrl+Right"
        enabled: Chilpa.playing
        onActivated: Chilpa.nextTrack()
    }
    Shortcut {
        sequence: "Ctrl+Left"
        enabled: Chilpa.playing
        onActivated: Chilpa.previousTrack()
    }
    Shortcut {
        sequence: StandardKey.MoveToNextChar
        enabled: !root.typing && Chilpa.playing
        onActivated: Chilpa.seek(Math.min(Chilpa.seconds + 10, Chilpa.totalSeconds))
    }
    Shortcut {
        sequence: StandardKey.MoveToPreviousChar
        enabled: !root.typing && Chilpa.playing
        onActivated: Chilpa.seek(Math.max(Chilpa.seconds - 10, 0))
    }
    // The volume moves in fives: one step should be audible, and a hundred
    // presses to cross the range is not a shortcut.
    Shortcut {
        sequence: StandardKey.MoveToPreviousLine
        enabled: !root.typing && Chilpa.hasVolume
        onActivated: Chilpa.chooseVolume(Math.min(Chilpa.volume + 5, 100))
    }
    Shortcut {
        sequence: StandardKey.MoveToNextLine
        enabled: !root.typing && Chilpa.hasVolume
        onActivated: Chilpa.chooseVolume(Math.max(Chilpa.volume - 5, 0))
    }
    Shortcut {
        sequence: "M"
        enabled: !root.typing && Chilpa.canMute
        onActivated: Chilpa.chooseMuted(!Chilpa.muted)
    }

    Button {
        text: Chilpa.translate("Previous", Chilpa.trRev)
        enabled: Chilpa.playing
        onClicked: Chilpa.previousTrack()
    }
    Button {
        text: Chilpa.paused ? Chilpa.translate("Resume", Chilpa.trRev) : Chilpa.translate("Pause", Chilpa.trRev)
        enabled: Chilpa.playing
        onClicked: Chilpa.pause(!Chilpa.paused)
    }
    Button {
        text: Chilpa.translate("Next", Chilpa.trRev)
        enabled: Chilpa.playing && Chilpa.queue.length > 1
        onClicked: Chilpa.nextTrack()
    }
    Button {
        text: Chilpa.translate("Stop", Chilpa.trRev)
        enabled: Chilpa.playing
        onClicked: Chilpa.stop()
    }

    // Shuffle and repeat change what comes next, not what is playing:
    // they can be touched with the queue running.
    Button {
        text: Chilpa.translate("Shuffle", Chilpa.trRev)
        checkable: true
        checked: Chilpa.shuffle
        onToggled: Chilpa.shuffle = checked
    }
    Button {
        text: [Chilpa.translate("Repeat: off", Chilpa.trRev), Chilpa.translate("Repeat: queue", Chilpa.trRev), Chilpa.translate("Repeat: track", Chilpa.trRev)][Chilpa.repeat]
        onClicked: Chilpa.cycleRepeat()
    }
    // The volume of the device itself, where the transport is. It only
    // appears when the output has one: Chilpa does not scale samples, so an
    // output without a control of its own has no volume to show. Why is in
    // the settings panel, which is where someone goes looking.
    RowLayout {
        visible: Chilpa.hasVolume
        spacing: 6
        Button {
            visible: Chilpa.canMute
            text: Chilpa.muted
                ? Chilpa.translate("Unmute", Chilpa.trRev)
                : Chilpa.translate("Mute", Chilpa.trRev)
            onClicked: Chilpa.chooseMuted(!Chilpa.muted)
        }
        Slider {
            Layout.preferredWidth: 110
            from: 0
            to: 100
            stepSize: 1
            value: Chilpa.volume
            ToolTip.visible: hovered
            ToolTip.text: Chilpa.translateArgs(
                "Volume: {}%", [String(Chilpa.volume)], Chilpa.trRev)
            onMoved: Chilpa.chooseVolume(Math.round(value))
        }
    }

    // It reorders what is left so the tempo does not jump. It needs the
    // library to have been analysed.
    Button {
        text: "AutoMix"
        enabled: Chilpa.queue.length > 2
        onClicked: Chilpa.automix()
    }
}
