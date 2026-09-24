// Previous, pause, next and stop, with their shortcuts.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import com.blitzfc.chilpa

RowLayout {
    id: root
    spacing: 8

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

    Button {
        text: "Anterior"
        enabled: Chilpa.playing
        onClicked: Chilpa.previousTrack()
    }
    Button {
        text: Chilpa.paused ? "Reanudar" : "Pausa"
        enabled: Chilpa.playing
        onClicked: Chilpa.pause(!Chilpa.paused)
    }
    Button {
        text: "Siguiente"
        enabled: Chilpa.playing && Chilpa.queue.length > 1
        onClicked: Chilpa.nextTrack()
    }
    Button {
        text: "Parar"
        enabled: Chilpa.playing
        onClicked: Chilpa.stop()
    }

    // Shuffle and repeat change what comes next, not what is playing:
    // they can be touched with the queue running.
    Button {
        text: "Aleatorio"
        checkable: true
        checked: Chilpa.shuffle
        onToggled: Chilpa.shuffle = checked
    }
    Button {
        text: ["Repetir: no", "Repetir: cola", "Repetir: pista"][Chilpa.repeat]
        onClicked: Chilpa.cycleRepeat()
    }
    // It reorders what is left so the tempo does not jump. It needs the
    // library to have been analysed.
    Button {
        text: "AutoMix"
        enabled: Chilpa.queue.length > 2
        onClicked: Chilpa.automix()
    }
}
