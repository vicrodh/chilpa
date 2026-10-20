import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

// The command line shares window actions and keeps an invalid line editable.
Rectangle {
    id: bar
    objectName: "commandBar"
    property bool opened: false
    property var returnFocus: null
    property string error: ""
    property var suggestions: []
    property int suggestion: -1
    property var history: []
    property int historyAt: 0
    property string draft: ""
    property var helpLines: []
    readonly property alias input: field
    signal execute(var command)
    signal dismissed(var previous)
    visible: opened
    height: Metrics.fieldHeight + Metrics.gap16
    color: Theme.color.surfaceRaised
    border.color: error !== "" ? Theme.color.statusNegative : Theme.color.strokeHairline
    function show(initial, previous) {
        returnFocus = previous
        opened = true
        error = ""
        helpLines = []
        history = JSON.parse(CommandLine.history() || "[]")
        historyAt = history.length
        field.text = initial
        field.forceActiveFocus()
        field.cursorPosition = field.text.length
        updateSuggestions()
    }
    function dismiss() {
        // The line lets go before the bar hides: the window's root is a focus
        // scope and would hand the keys back to it, and the next colon would
        // be typed into a line nobody sees.
        field.focus = false
        opened = false
        dismissed(returnFocus)
    }
    // While it is open the console holds the keys, so Escape always reaches
    // it; when something else takes them (a search result chosen with
    // Enter, a click elsewhere) it closes and leaves them there. A window
    // that only stops being the active one keeps it open.
    function lost() {
        if (!opened || field.activeFocus || !bar.Window.active) return
        field.focus = false
        opened = false
        helpLines = []
    }
    function updateSuggestions() {
        error = ""
        suggestion = -1
        suggestions = JSON.parse(CommandLine.complete(field.text) || "[]")
    }
    function complete(by) {
        if (!suggestions.length) return
        suggestion = suggestion < 0 ? (by > 0 ? 0 : suggestions.length - 1) : (suggestion + by + suggestions.length) % suggestions.length
        const choices = suggestions
        field.text = choices[suggestion]
        suggestions = choices
        field.cursorPosition = field.text.length
    }
    function recall(by) {
        if (historyAt === history.length) draft = field.text
        historyAt = Math.max(0, Math.min(history.length, historyAt + by))
        field.text = historyAt === history.length ? draft : history[historyAt]
        field.cursorPosition = field.text.length
    }
    function submit() {
        const command = JSON.parse(CommandLine.prepare(field.text) || "{}")
        if (command.error) { error = command.error; return }
        if (command.action === "commands.help") {
            helpLines = command.lines || []
            suggestions = []
            field.text = ""
            return
        }
        dismiss()
        execute(command)
    }
    Column {
        anchors.bottom: parent.top
        width: parent.width
        visible: bar.opened && bar.suggestions.length > 0 && bar.error === "" && !bar.helpLines.length
        Repeater {
            model: bar.suggestions.slice(0, 6)
            Rectangle {
                required property string modelData
                required property int index
                width: bar.width
                height: Metrics.fieldHeight
                color: index === bar.suggestion ? Theme.color.fillField : Theme.color.surfaceRaised
                Text {
                    anchors.fill: parent
                    anchors.margins: Metrics.gap8
                    text: ":" + modelData
                    color: Theme.color.textSecondary
                    font.pointSize: Type.body
                    font.family: Type.consoleFamily
                }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: { field.text = modelData; field.forceActiveFocus() } }
            }
        }
    }
    Loader {
        anchors.bottom: parent.top
        width: parent.width
        active: bar.opened && bar.helpLines.length > 0
        sourceComponent: CommandHelp { lines: bar.helpLines; room: Math.max(0, bar.y) }
    }
    Text {
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap12
        anchors.verticalCenter: field.verticalCenter
        text: ":"
        color: Theme.color.accent
        font.pointSize: Type.body
        font.family: Type.consoleFamily
    }
    TextInput {
        id: field
        objectName: "commandInput"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.gap12
        anchors.leftMargin: Metrics.gap24
        height: Metrics.fieldHeight - Metrics.gap12
        color: Theme.color.textPrimary
        selectionColor: Theme.color.accent
        font.pointSize: Type.body
        font.family: Type.consoleFamily
        clip: true
        selectByMouse: true
        onTextEdited: { bar.updateSuggestions(); bar.historyAt = bar.history.length }
        onActiveFocusChanged: if (!activeFocus) Qt.callLater(bar.lost)
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Escape) { if (bar.helpLines.length) bar.helpLines = []; else bar.dismiss() }
            else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) bar.submit()
            else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) bar.complete(event.modifiers & Qt.ShiftModifier || event.key === Qt.Key_Backtab ? -1 : 1)
            else if (event.key === Qt.Key_Up) bar.recall(-1)
            else if (event.key === Qt.Key_Down) bar.recall(1)
            else return
            event.accepted = true
        }
    }
    Text {
        objectName: "commandError"
        anchors.left: field.left
        anchors.right: field.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap4
        text: bar.error
        elide: Text.ElideRight
        color: Theme.color.statusNegative
        font.pointSize: Type.meta
        font.family: Type.consoleFamily
    }
}
