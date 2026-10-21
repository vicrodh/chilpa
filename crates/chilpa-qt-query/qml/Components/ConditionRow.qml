import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// One condition (F4): the field with its glyph, how it matches, its values
// as one joined block, a + while it takes more values, and the bin. At
// least 34 tall, radius 8, lit while hovered or while its picker is open.
// Narrow, the field sits above the rest (F7).
Rectangle {
    id: line
    property var row: ({})
    property bool picking: false
    property bool narrow: false
    property bool longLabels: false
    signal removed()
    signal operatorChosen(string key)
    signal valueRemoved(string value)
    signal pickAsked()

    readonly property real labelRoom: narrow ? width - Refine.rowPadLeft - Refine.rowPadRight : longLabels ? Refine.labelWideLong : Refine.labelWide
    readonly property real middleX: narrow ? Refine.rowPadLeft : Refine.rowPadLeft + labelRoom + Refine.rowApartX
    readonly property real middleY: narrow ? Refine.rowPadTop + Refine.opTall + Refine.rowApartY : Refine.rowPadTop
    readonly property real middleRoom: width - middleX - Refine.rowButton - Refine.rowApartX - Refine.rowPadRight

    implicitHeight: Math.max(Refine.rowMinTall, middleY + middle.implicitHeight + Refine.rowPadTop)
    radius: Refine.rowRound
    color: picking || hover.hovered ? Theme.color.stateHover : "transparent"

    HoverHandler { id: hover }

    Row {
        objectName: "conditionLabel"
        x: Refine.rowPadLeft
        y: Refine.rowPadTop
        width: line.labelRoom
        height: Refine.opTall
        spacing: Refine.labelApart
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: RefineText.glyph(line.row.field)
            size: Refine.labelIcon
            color: Theme.color.textTertiary
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: RefineText.field(line.row.field)
            font.pointSize: Type.row
            color: Theme.color.textSecondary
            elide: Text.ElideRight
        }
    }
    Flow {
        id: middle
        x: line.middleX
        y: line.middleY
        width: line.middleRoom
        spacing: Refine.innerApart
        OperatorSwitch {
            objectName: "conditionOperator"
            row: line.row
            onChosen: (key) => line.operatorChosen(key)
        }
        ValueGroup {
            objectName: "conditionValues"
            tokens: RefineText.tokens(line.row)
            negated: line.row.kind === "list" && !!line.row.negated
            room: line.middleRoom
            onPicked: line.pickAsked()
            onRemoved: (value) => line.valueRemoved(value)
        }
        SquareButton {
            objectName: "conditionAdd"
            visible: line.row.kind === "list" || line.row.kind === "words"
            small: true
            glyph: Icons.plus
            tip: I18n.tr("Add values", I18n.rev)
            onClicked: line.pickAsked()
        }
    }
    SquareButton {
        objectName: "conditionRemove"
        anchors.right: parent.right
        anchors.rightMargin: Refine.rowPadRight
        y: Refine.rowPadTop
        small: true
        glyph: Icons.trash
        tint: Theme.color.textTertiary
        tip: I18n.tr("Remove condition", I18n.rev)
        onClicked: line.removed()
    }
}
