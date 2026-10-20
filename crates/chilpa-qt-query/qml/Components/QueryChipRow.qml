import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The Library's conditions beyond its columns, one chip each: `:filter`
// writes them, and Alt+click on a column's value keeps that value out. A
// chip turns around when its words are clicked (kept in, kept out) and goes
// away with its x; with two or more, "All of" or "Any of" says how they
// join, and a click swaps it. What the chip says in the command line's
// words shows under the pointer.
Row {
    id: chips

    // The query as text; `edited` hands back the one a click made.
    property string query: Library.queryText
    readonly property var parsed: {
        I18n.rev
        return JSON.parse(QueryChips.read(chips.query) || "{\"any\":false,\"chips\":[]}")
    }
    signal edited(string query)

    visible: parsed.chips.length > 0
    spacing: Metrics.gap6

    Text {
        objectName: "queryJoin"
        visible: chips.parsed.chips.length > 1
        anchors.verticalCenter: parent.verticalCenter
        text: chips.parsed.any ? I18n.tr("Any of", I18n.rev) : I18n.tr("All of", I18n.rev)
        font.pointSize: Type.meta
        font.weight: Type.medium
        color: Theme.color.accent
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: chips.edited(QueryChips.joined(chips.query, !chips.parsed.any)) }
    }

    Repeater {
        model: chips.parsed.chips

        Rectangle {
            id: chip
            required property var modelData
            required property int index
            objectName: "queryChip"
            anchors.verticalCenter: parent.verticalCenter
            width: line.implicitWidth + Metrics.gap10 + Metrics.gap4 // derived: the pill's padding
            height: Metrics.crumbHeight
            radius: height / 2 // derived: a pill
            color: modelData.negated ? Theme.color.fillField : Theme.color.stateSelected
            border.width: modelData.negated ? Metrics.hairline : 0
            border.color: Theme.color.statusNegative

            Row {
                id: line
                anchors.left: parent.left
                anchors.leftMargin: Metrics.gap10
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap4

                Icon {
                    visible: chip.modelData.negated
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.minus
                    size: Metrics.iconSm
                    color: Theme.color.statusNegative
                }
                Text {
                    objectName: "queryChipLabel"
                    anchors.verticalCenter: parent.verticalCenter
                    text: chip.modelData.label
                    font.pointSize: Type.meta
                    font.weight: Type.medium
                    font.strikeout: chip.modelData.negated
                    color: Theme.color.textPrimary
                    HoverHandler {
                        cursorShape: Qt.PointingHandCursor
                        onHoveredChanged: hovered ? TipState.show(chip.modelData.text, chip) : TipState.hide(chip)
                    }
                    TapHandler { onTapped: chips.edited(QueryChips.flipped(chips.query, chip.index)) }
                    Component.onDestruction: TipState.hide(chip)
                }
                Icon {
                    objectName: "queryChipRemove"
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.x
                    size: Metrics.iconSm
                    color: Theme.color.textSecondary
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: chips.edited(QueryChips.without(chips.query, chip.index)) }
                }
            }
        }
    }
}
