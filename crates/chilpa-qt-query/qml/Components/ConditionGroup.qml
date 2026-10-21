import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// A group of conditions with its own all / any (F5): a box on the window's
// colour inside the panel, its head with brackets, the switch, what it means,
// then + (new conditions go in here; the box is ringed in the accent while
// they do), ungroup and the bin. Empty, it says where the next ones go.
Rectangle {
    id: box
    property var group: ({ rows: [] })
    property bool target: false
    property bool narrow: false
    property bool longLabels: false
    property string picking: ""
    signal modeChosen(string mode)
    signal targetToggled()
    signal ungrouped()
    signal removed()
    signal rowRemoved(string id)
    signal rowOperator(string id, string key)
    signal rowValueRemoved(string id, string value)
    signal rowPickAsked(string field)

    readonly property var rows: group.rows || []
    implicitHeight: content.implicitHeight + Refine.groupPadTop + Refine.groupPadBottom
    radius: Refine.groupRound
    color: Theme.color.surfaceWindow
    border.width: target ? Refine.targetRing : Metrics.hairline
    border.color: target ? Theme.color.accent : Theme.color.strokeHairline

    Column {
        id: content
        x: Refine.groupPadLeft
        y: Refine.groupPadTop
        width: box.width - Refine.groupPadLeft - Refine.groupPadRight
        spacing: Refine.rowsApart
        Item {
            width: parent.width
            height: Refine.groupHead
            Row {
                x: Refine.groupHeadLeft
                anchors.verticalCenter: parent.verticalCenter
                spacing: Refine.headApart
                Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.brackets; size: Refine.labelIcon; color: Theme.color.textTertiary }
                Segments {
                    objectName: "groupMatch"
                    anchors.verticalCenter: parent.verticalCenter
                    tall: Refine.matchSegTall
                    pad: Refine.matchSegPad
                    options: [{ text: I18n.tr("all", I18n.rev) }, { text: I18n.tr("any", I18n.rev) }]
                    current: box.group.mode === "any" ? 1 : 0
                    onActivated: (index) => box.modeChosen(index === 1 ? "any" : "all")
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: box.group.mode === "any" ? I18n.tr("Any of these", I18n.rev) : I18n.tr("All of these", I18n.rev)
                    font.pointSize: Type.meta
                    color: Theme.color.textSecondary
                }
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                SquareButton { objectName: "groupTarget"; small: true; glyph: Icons.plus; checked: box.target; tip: I18n.tr("New conditions go into this group", I18n.rev); onClicked: box.targetToggled() }
                SquareButton { objectName: "groupUngroup"; small: true; glyph: Icons.bracketsOff; tip: I18n.tr("Ungroup", I18n.rev); onClicked: box.ungrouped() }
                SquareButton { objectName: "groupRemove"; small: true; glyph: Icons.trash; tint: Theme.color.textTertiary; tip: I18n.tr("Remove group", I18n.rev); onClicked: box.removed() }
            }
        }
        Text {
            visible: box.rows.length === 0
            leftPadding: Refine.emptyGroupPad
            bottomPadding: Refine.emptyGroupPad
            text: I18n.tr("Empty group. Conditions you add now go here.", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
        Repeater {
            model: box.rows
            ConditionRow {
                required property var modelData
                width: content.width
                row: modelData
                narrow: box.narrow
                longLabels: box.longLabels
                picking: box.picking === modelData.field
                onRemoved: box.rowRemoved(modelData.id)
                onOperatorChosen: (key) => box.rowOperator(modelData.id, key)
                onValueRemoved: (value) => box.rowValueRemoved(modelData.id, value)
                onPickAsked: box.rowPickAsked(modelData.field)
            }
        }
    }
}
