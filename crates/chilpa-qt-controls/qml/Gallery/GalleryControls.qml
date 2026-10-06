import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Components"
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
    ShortcutsSheet {}
    ShortcutsEditor {}
    SleepTimer {}
    WindowControls { window: Window.window }
}
