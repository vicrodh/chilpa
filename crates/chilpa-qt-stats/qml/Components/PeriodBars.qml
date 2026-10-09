import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The period a bar at a time (a day, or a month for a year and for all of
// it), each over a faint bar of the same place in the period before. Under
// the bars, their labels, as many as fit; a bar under the pointer says its
// day and its count, and the others dim around it.
Item {
    id: chart

    // [{label, tip, plays, prev}]
    property var points: []
    property color tint: Theme.color.accent
    // The bar under the pointer, or -1.
    property int hovered: -1
    readonly property int most: points.reduce((m, p) => Math.max(m, p.plays, p.prev), 1)
    readonly property real step: points.length > 0 ? width / points.length : 0
    readonly property int labelEvery: Math.max(1, Math.ceil(Charts.barLabelWide / Math.max(1, step)))
    readonly property real room: height - Charts.barLabel

    Repeater {
        model: chart.points

        Item {
            id: slot
            required property var modelData
            required property int index
            x: index * chart.step
            width: chart.step
            height: chart.height

            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                y: chart.room - height
                width: Math.max(1, slot.width - 2 * Charts.barApart) // derived: a gap on both sides
                height: chart.room * slot.modelData.prev / chart.most
                radius: Math.min(Charts.barRound, width / 2) // derived: rounded ends
                color: Theme.color.statsPrevious
            }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                y: chart.room - height
                width: Math.max(1, slot.width - 2 * Charts.barApart) // derived: a gap on both sides
                height: Math.max(slot.modelData.plays > 0 ? Charts.barApart : 0, chart.room * slot.modelData.plays / chart.most)
                radius: Math.min(Charts.barRound, width / 2) // derived: rounded ends
                color: chart.tint
                opacity: chart.hovered < 0 || chart.hovered === slot.index ? 1 : Metrics.disabledOpacity
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                y: chart.room + Metrics.gap4
                visible: slot.index % chart.labelEvery === 0
                text: slot.modelData.label
                font.pointSize: Type.micro
                color: Theme.color.textTertiary
            }
            HoverHandler {
                onHoveredChanged: {
                    if (hovered) {
                        chart.hovered = slot.index
                        TipState.show(I18n.trArgs("{} · {}", [slot.modelData.tip, I18n.trCount("{} play", "{} plays", slot.modelData.plays, I18n.rev)], I18n.rev), slot)
                    } else {
                        if (chart.hovered === slot.index)
                            chart.hovered = -1
                        TipState.hide(slot)
                    }
                }
            }
        }
    }
}
