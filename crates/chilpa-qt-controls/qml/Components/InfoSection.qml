import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One section of what is known of a track: a rule, a heading, and its cells
// in as many columns as fit, three at most. A fact is a label over its value,
// a credit a name over its roles, and a place on disk or on a server a label
// over a selectable code block with its note. Text wraps; nothing is cut.
Column {
    id: section

    property string title
    // Facts and places: [{label, value, note}]. Credits: [{name, roles}].
    property var cells: []
    property bool credits: false
    // Values in a fixed-width face, one per row: a path or a server entry.
    property bool code: false
    property int maxColumns: 3
    readonly property int columns: code ? 1 : Math.max(1, Math.min(maxColumns, Math.floor((width + Metrics.gap16) / (Metrics.trackInfoCell + Metrics.gap16))))

    visible: cells.length > 0
    spacing: Metrics.gap10

    Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    Text {
        objectName: "infoSectionTitle"
        width: parent.width
        text: section.title
        wrapMode: Text.Wrap
        font.pointSize: Type.meta
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }
    Grid {
        id: grid
        objectName: "infoSectionCells"
        width: parent.width
        columns: section.columns
        columnSpacing: Metrics.gap16
        rowSpacing: Metrics.gap12
        readonly property real cellWidth: Math.max(0, (width - columnSpacing * (columns - 1)) / columns)
        Repeater {
            model: section.cells
            Column {
                required property var modelData
                width: grid.cellWidth
                spacing: Metrics.gap2
                Text {
                    width: parent.width
                    visible: text !== ""
                    text: section.credits ? "" : modelData.label || ""
                    wrapMode: Text.Wrap
                    font.pointSize: Type.label
                    color: Theme.color.textTertiary
                }
                TextEdit {
                    objectName: "infoSectionValue"
                    width: parent.width
                    visible: !section.code
                    text: section.credits ? modelData.name || "" : modelData.value || ""
                    textFormat: TextEdit.PlainText
                    readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                    selectionColor: Theme.color.accent
                    font.pointSize: Type.small
                    color: Theme.color.textPrimary
                }
                TextEdit {
                    objectName: "infoSectionCode"
                    width: parent.width
                    visible: section.code
                    text: section.code ? modelData.value || "" : ""
                    textFormat: TextEdit.PlainText
                    readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere
                    selectionColor: Theme.color.accent
                    font.family: Type.consoleFamily
                    font.pointSize: Type.meta
                    color: Theme.color.textSecondary
                }
                Text {
                    width: parent.width
                    visible: text !== ""
                    text: section.credits ? modelData.roles || "" : modelData.note || ""
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font.pointSize: Type.label
                    color: section.credits ? Theme.color.textSecondary : Theme.color.textTertiary
                }
            }
        }
    }
}
