import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I carry a search target across settings pages and reveal its loaded row.
Item {
    id: focus
    required property string section
    required property var viewport
    required property var model
    required property var highlight
    property bool leaving: false
    property string key: ""
    property int row: -1
    signal located()

    function go(setting) {
        key = setting.key
        row = -1
        highlight.target = null
        Visit.keep("settings.focus", setting.section !== section ? setting : null)
        Navigation.openSection(setting.section)
        seeking.tries = 0
        if (setting.section === section) seeking.restart()
    }
    onLeavingChanged: if (leaving) {
        const target = Visit.recall("settings.focus", null)
        if (target && (Navigation.screen !== "settings" || Navigation.section !== target.section))
            Visit.keep("settings.focus", null)
    }
    Component.onCompleted: {
        const target = Visit.recall("settings.focus", null)
        if (target?.section === section) {
            key = target.key
            Visit.keep("settings.focus", null)
            seeking.restart()
        }
    }
    Timer {
        id: seeking
        property int tries: 0
        interval: Motion.fast
        repeat: true
        onTriggered: {
            tries += 1
            for (let i = 0; i < focus.viewport.count && focus.row < 0; i++)
                if (focus.model.data(focus.model.index(i, 0), Metrics.settingsKeyRole) === focus.key)
                    focus.row = i
            if (focus.row >= 0) {
                focus.viewport.positionViewAtIndex(focus.row, ListView.Center)
                focus.highlight.target = focus.viewport.itemAtIndex(focus.row)
            }
            if (focus.highlight.target || tries >= Metrics.settingsSeekTries) {
                stop()
                if (focus.highlight.target) focus.located()
            }
        }
    }
}
