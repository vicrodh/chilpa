import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"
import "../Patterns"

TitledFlow {
    title: "Controls"
    Keycap { text: "Ctrl + ▶" }
    Keycap { text: "Press keys…"; button: true; recording: true }
    ShortcutRow { width: Metrics.confirmWidth; entry: ({ id: "playback.next", label: "Next Track", shortcut: "Ctrl + ▶", modified: false }) }
    ShortcutRow { width: Metrics.confirmWidth; editing: true; recording: true; tried: "Q"; takenBy: "Queue"; entry: ({ id: "playback.mute", label: "Mute / Unmute", shortcut: "M", modified: true }) }
    ShortcutColumns { width: Metrics.settingsMaxWidth; keys: ({ col1: [{ label: "Playback", rows: [{ id: "playback.toggle", label: "Play / Pause", shortcut: "Space", modified: false }] }] }) }
    LoopRow { width: Metrics.playbackOptionsWidth }
    LoopButton {}
    QueueFooter { width: Metrics.rightPanel }
    QueueEnds { summary: "12 songs 1 hr 5 min"; remaining: 65 * 60000 } // derived: an hour and five minutes
    ShortcutsSheet {}
    ShortcutsEditor {}
    SleepTimer {}
    QuitQuestion {}
    WindowControls { window: Window.window }
}
