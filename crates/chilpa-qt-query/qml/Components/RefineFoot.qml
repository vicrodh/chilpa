import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// Where conditions are added (F2, F3): "Add a condition…" grows to the room
// the field chips leave it (at least 240, as the design's flex basis), the
// chips of each field after it (one inverted while its values are open),
// "Group" once there is something to group, and narrow a single "Fields"
// in their place (F7). Under it, whatever the panel opens: suggestions or
// a field's values.
Item {
    id: foot
    property string picking: ""
    property bool narrow: false
    property bool canGroup: false
    property alias typed: input.text
    property alias extra: below.sourceComponent
    readonly property alias extraItem: below.item
    signal fieldChosen(string field)
    signal fieldsAsked()
    signal groupAsked()
    signal accepted(bool exclude)
    signal escaped()

    implicitHeight: line.height + (below.item ? Refine.footApart + below.height : 0)

    function focusInput() { input.forceActiveFocus() }

    Item {
        id: line
        width: parent.width
        // The chips side by side, summed from each chip: a Flow's own width
        // follows the width it is given.
        readonly property real chipsWide: {
            let total = 0
            let shown = 0
            for (const chip of chips.children)
                if (chip.visible && chip.implicitWidth > 0) { total += chip.implicitWidth; shown += 1 }
            return total + Math.max(0, shown - 1) * Refine.fieldChipsApart
        }
        readonly property bool together: Refine.addMinWide + Refine.footApart + chipsWide <= width
        height: together ? Math.max(field.height, chips.height) : field.height + Refine.footApart + chips.height

        Rectangle {
            id: field
            objectName: "refineAdd"
            width: line.together ? line.width - line.chipsWide - Refine.footApart : line.width
            height: Refine.addTall
            radius: Refine.addRound
            color: Theme.color.surfaceWindow
            border.width: input.activeFocus || input.text !== "" ? Refine.addRing : Metrics.hairline
            border.color: input.activeFocus || input.text !== "" ? Theme.color.accent : Theme.color.strokeHairline
            Row {
                x: Refine.addPad
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - Refine.addPad * 2 // derived: padding on both sides
                spacing: Refine.labelApart
                Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.plus; size: Refine.addIcon; color: Theme.color.textSecondary }
                TextInput {
                    id: input
                    objectName: "refineAddInput"
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - Refine.addIcon - Refine.labelApart * 2 - (clearInput.visible ? clearInput.width : 0) // derived: what the glyph and the x leave
                    clip: true
                    font.pointSize: Type.row
                    color: Theme.color.textPrimary
                    selectionColor: Theme.color.rowSelected
                    Keys.onReturnPressed: (event) => foot.accepted((event.modifiers & Qt.ShiftModifier) !== 0)
                    Keys.onEnterPressed: (event) => foot.accepted((event.modifiers & Qt.ShiftModifier) !== 0)
                    Keys.onEscapePressed: { input.text = ""; foot.escaped() }
                    Text {
                        anchors.fill: parent
                        visible: input.text === ""
                        text: foot.narrow ? I18n.tr("Add a condition…", I18n.rev) : I18n.tr("Add a condition: type a genre, artist, year…", I18n.rev)
                        font: input.font
                        color: Theme.color.textTertiary
                        elide: Text.ElideRight
                    }
                }
                SquareButton {
                    id: clearInput
                    visible: input.text !== ""
                    anchors.verticalCenter: parent.verticalCenter
                    width: Refine.addClear
                    height: Refine.addClear
                    small: true
                    glyph: Icons.x
                    tip: I18n.tr("Clear", I18n.rev)
                    onClicked: input.text = ""
                }
            }
        }
        Flow {
            id: chips
            x: line.together ? field.width + Refine.footApart : 0
            y: line.together ? (field.height - height) / 2 : field.height + Refine.footApart // derived: centred beside the field
            width: line.together ? line.chipsWide : line.width
            spacing: Refine.fieldChipsApart
            Repeater {
                model: foot.narrow ? [] : RefineText.order
                FieldChip {
                    required property string modelData
                    objectName: "fieldChip"
                    glyph: RefineText.glyph(modelData)
                    text: RefineText.field(modelData)
                    active: foot.picking === modelData
                    onClicked: foot.fieldChosen(modelData)
                }
            }
            FieldChip {
                objectName: "fieldsChip"
                visible: foot.narrow
                quiet: true
                glyph: Icons.list
                text: I18n.tr("Fields", I18n.rev)
                onClicked: foot.fieldsAsked()
            }
            FieldChip {
                objectName: "groupChip"
                visible: foot.canGroup
                quiet: true
                glyph: Icons.brackets
                text: I18n.tr("Group", I18n.rev)
                onClicked: foot.groupAsked()
            }
        }
    }
    Loader {
        id: below
        anchors.top: line.bottom
        anchors.topMargin: Refine.footApart
        width: parent.width
    }
}
