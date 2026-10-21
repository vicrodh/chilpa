import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// The query as the console reads it (F6, F9): hidden until asked for, then
// one monospace line to copy or edit. Editing, the field is ringed in the
// accent, or in the negative colour with the character the engine stopped
// at marked and its message under it; Enter applies a query that reads.
Rectangle {
    id: line
    // The query, without ":filter".
    property string query: ""
    property bool editing: false
    signal applied(string query)

    readonly property string prefix: ":filter "
    readonly property var verdict: editing ? JSON.parse(QueryChips.check(input.text.replace(/^:filter\s*/, "")) || "{\"ok\":true}") : ({ ok: true })
    readonly property int errorAt: verdict.ok ? -1 : verdict.at + prefix.length

    function escaped(text) { return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    function marked(text, at) {
        const ch = text.charAt(at) || " "
        return escaped(text.slice(0, at)) + "<span style=\"background-color:" + Theme.color.stateSelected + ";color:" + Theme.color.statusNegative + ";text-decoration:underline\">" + escaped(ch) + "</span>" + escaped(text.slice(at + 1))
    }
    function edit() { input.text = prefix + query; editing = true; input.forceActiveFocus() }
    function apply() { if (verdict.ok) { editing = false; line.applied(input.text.replace(/^:filter\s*/, "")) } }

    implicitHeight: Math.max(Refine.button, content.implicitHeight) + Refine.consolePadTop + Refine.headApart
    color: Theme.color.surfaceWindow
    Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    Clipboard { id: clipboard }

    Icon {
        x: Refine.headLeft
        y: Refine.consolePadTop + Refine.chipApart
        glyph: Icons.terminal2
        size: Refine.labelIcon
        color: Theme.color.textTertiary
    }
    Column {
        id: content
        x: Refine.headLeft + Refine.labelIcon + Refine.headApart
        y: Refine.consolePadTop
        width: parent.width - x - Refine.button * 2 - Refine.headApart - Refine.headRight // derived: room for the copy and edit buttons
        spacing: Refine.chipApart
        Controls.ScrollView {
            visible: !line.editing
            width: parent.width
            height: Math.min(view.implicitHeight + Refine.chipApart * 2, Refine.consoleMaxTall) // derived: the line's padding
            Text {
                id: view
                objectName: "consoleText"
                topPadding: Refine.chipApart
                width: content.width
                text: line.prefix + line.query
                font.family: Type.consoleFamily
                font.pointSize: Type.meta
                lineHeight: 1.5
                color: Theme.color.textSecondary
                wrapMode: Text.WrapAnywhere
            }
        }
        Rectangle {
            visible: line.editing
            width: parent.width
            height: Refine.consoleField
            radius: Refine.pickSearchRound
            color: Theme.color.fillField
            border.width: Refine.targetRing
            border.color: line.verdict.ok ? Theme.color.accent : Theme.color.statusNegative
            TextInput {
                id: input
                objectName: "consoleInput"
                anchors.fill: parent
                anchors.leftMargin: Refine.pickPad
                anchors.rightMargin: Refine.pickPad
                verticalAlignment: TextInput.AlignVCenter
                clip: true
                font.family: Type.consoleFamily
                font.pointSize: Type.meta
                color: Theme.color.textPrimary
                Keys.onReturnPressed: line.apply()
                Keys.onEnterPressed: line.apply()
                Keys.onEscapePressed: line.editing = false
            }
        }
        Text {
            objectName: "consoleMarked"
            visible: line.editing && !line.verdict.ok
            width: parent.width
            textFormat: Text.RichText
            text: line.errorAt < 0 ? "" : line.marked(input.text, line.errorAt)
            font.family: Type.consoleFamily
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
            wrapMode: Text.WrapAnywhere
        }
        Row {
            visible: line.editing && !line.verdict.ok
            spacing: Refine.innerApart
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.alertCircle; size: Refine.rowButtonIcon; color: Theme.color.statusNegative }
            Text { objectName: "consoleError"; anchors.verticalCenter: parent.verticalCenter; text: line.verdict.ok ? "" : line.errorAt + ": " + line.verdict.message; font.pointSize: Type.meta; color: Theme.color.statusNegative }
        }
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: Refine.headRight
        y: Refine.consolePadTop
        SquareButton { objectName: "consoleCopy"; glyph: Icons.copy; tip: I18n.tr("Copy", I18n.rev); onClicked: clipboard.write(line.editing ? input.text : line.prefix + line.query) }
        SquareButton {
            objectName: "consoleEdit"
            glyph: line.editing ? Icons.check : Icons.pencil
            tip: line.editing ? I18n.tr("Apply", I18n.rev) : I18n.tr("Edit as text", I18n.rev)
            onClicked: line.editing ? line.apply() : line.edit()
        }
    }
}
