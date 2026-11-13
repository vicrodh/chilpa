import QtQuick
import QtQuick.Controls.Basic as Controls
import "../Theme"
import "../Primitives"

// Scroll requests follow user direction; model changes never trigger prefetch.
ListView {
    id: choices
    required property TrackChoiceWindow choiceWindow
    property bool surfaceOpen: false
    property string field: ""
    property var draft: []
    property var formatter
    signal toggled(string key)
    signal accepted()
    objectName: "tracksFilterChoices"
    property var snapshot: null
    property int shift: 0
    property bool restoring: false
    property real lastY: 0
    function capture() {
        const index = Math.max(0, indexAt(Metrics.gap2, contentY + Metrics.gap2))
        const item = itemAtIndex(index)
        snapshot = {index, offset:item ? item.y - contentY : 0, current:currentIndex}
        shift = 0; restoring = true
    }
    function restore() {
        if (!snapshot) return
        forceLayout()
        const index = Math.max(0, Math.min(count - 1, snapshot.index + shift))
        currentIndex = Math.max(-1, Math.min(count - 1, snapshot.current + shift))
        positionViewAtIndex(index, ListView.Beginning)
        const item = itemAtIndex(index)
        if (item) contentY = item.y - snapshot.offset
        snapshot = null; restoring = false; lastY = contentY
    }
    function prefetch(direction) {
        if (!choices.surfaceOpen || choiceWindow.loading || choiceWindow.error || restoring) return
        const edge = Metrics.menuItem * 3 // derived: request three rows before the loaded edge
        if (direction > 0 && choiceWindow.more && contentY + height >= originY + contentHeight - edge) choiceWindow.loadNext()
        else if (direction < 0 && choiceWindow.previous && contentY <= originY + edge) choiceWindow.loadPrevious()
    }
    clip: true
    reuseItems: true
    activeFocusOnTab: true
    model: surfaceOpen ? choiceWindow.rows : null
    boundsBehavior: Flickable.StopAtBounds
    onContentYChanged: {
        if (!restoring && (moving || bar.pressed)) prefetch(contentY >= lastY ? 1 : -1)
        lastY = contentY
    }
    Controls.ScrollBar.vertical: Controls.ScrollBar { id: bar }
    WheelHandler {
        target: null
        onWheel: event => { const direction = event.angleDelta.y < 0 ? 1 : -1; event.accepted = false; Qt.callLater(() => choices.prefetch(direction)) }
    }
    Keys.onReturnPressed: choices.accepted()
    Keys.onEnterPressed: choices.accepted()
    Keys.onSpacePressed: { const item = choiceWindow.optionAt(currentIndex); if (item) choices.toggled(item.key) }
    Keys.onDownPressed: { currentIndex = Math.min(count - 1, currentIndex + 1); positionViewAtIndex(currentIndex, ListView.Contain); prefetch(1) }
    Keys.onUpPressed: { currentIndex = Math.max(0, currentIndex - 1); positionViewAtIndex(currentIndex, ListView.Contain); prefetch(-1) }
    delegate: MenuEntry {
        required property string key
        required property string label
        required property int count
        required property int index
        width: choices.width
        entry: ({text: choices.formatter.value(choices.field, key, label), hint: String(count), checked: choices.draft.indexOf(key) >= 0, glyph: choices.field === "rating" && key !== "0" ? Icons.starFilled : "", filled: choices.field === "rating", focused: choices.activeFocus && index === choices.currentIndex})
        Accessible.role: Accessible.CheckBox
        Accessible.name: entry.text
        Accessible.checked: entry.checked
        onClicked: choices.toggled(key)
    }
}
