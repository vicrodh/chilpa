import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// How a condition matches (F4): a quiet pill ("is any of", "between", "at
// least"), a minus first when it keeps things out, and a chevron when it can
// change. A click lays its choices out in place; picking one sends it.
Item {
    id: op
    property var row: ({})
    property bool open: false
    readonly property var choices: RefineText.operators(row)
    readonly property bool negated: row.kind === "list" && !!row.negated
    signal chosen(string key)

    implicitWidth: open ? segments.implicitWidth : pill.implicitWidth
    implicitHeight: Refine.opTall

    Rectangle {
        id: pill
        objectName: "operatorPill"
        visible: !op.open
        implicitWidth: line.implicitWidth + Refine.opPadLeft + Refine.opPadRight
        height: Refine.opTall
        radius: Refine.opRound
        color: Theme.color.fillField
        Row {
            id: line
            x: Refine.opPadLeft
            anchors.verticalCenter: parent.verticalCenter
            spacing: Refine.opApart
            Icon {
                visible: op.negated
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.minus
                size: Refine.opMinus
                color: Theme.color.textPrimary
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: RefineText.operatorText(op.row)
                font.pointSize: Type.row
                color: op.negated ? Theme.color.textPrimary : Theme.color.textSecondary
            }
            Icon {
                visible: op.choices.length > 0
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.chevronDown
                size: Refine.opChevron
                color: Theme.color.textTertiary
            }
        }
        MouseArea {
            anchors.fill: parent
            enabled: op.choices.length > 0
            cursorShape: Qt.PointingHandCursor
            onClicked: op.open = true
        }
    }
    Segments {
        id: segments
        objectName: "operatorChoices"
        visible: op.open
        anchors.verticalCenter: parent.verticalCenter
        options: op.choices
        current: op.choices.findIndex(c => c.key === RefineText.operator(op.row))
        onActivated: (index) => {
            op.open = false
            op.chosen(op.choices[index].key)
        }
    }
}
