import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Bars that stand, a count a column, with its label under it and its
// count over it; the highest is drawn whole and the others a little
// fainter. For the days of the week and the decades.
Item {
    id: chart

    // [{label, value}]
    property var points: []
    property color tint: Theme.color.accent
    readonly property int most: points.reduce((m, p) => Math.max(m, p.value), 0)
    readonly property real step: points.length > 0 ? width / points.length : 0
    readonly property real room: height - 2 * Charts.barLabel // derived: a label over and under the bars

    Repeater {
        model: chart.points

        Item {
            id: column
            required property var modelData
            x: index * chart.step
            width: chart.step
            height: chart.height
            required property int index

            Rectangle {
                id: bar
                anchors.horizontalCenter: parent.horizontalCenter
                y: Charts.barLabel + chart.room - height
                width: Math.max(1, Math.min(Charts.thumb, column.width - 2 * Metrics.gap6)) // derived: a gap on both sides
                height: chart.most > 0 ? Math.max(column.modelData.value > 0 ? Charts.barApart : 0, chart.room * column.modelData.value / chart.most) : 0
                radius: Math.min(Charts.barRound, width / 2) // derived: rounded ends
                color: chart.tint
                opacity: column.modelData.value === chart.most ? 1 : Metrics.disabledOpacity + 0.25
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: bar.top
                anchors.bottomMargin: Metrics.gap2
                visible: column.modelData.value > 0
                text: column.modelData.value
                font.pointSize: Type.micro
                font.features: { "tnum": 1 }
                color: column.modelData.value === chart.most ? Theme.color.textPrimary : Theme.color.textTertiary
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                y: Charts.barLabel + chart.room + Metrics.gap4
                width: column.width
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                text: column.modelData.label
                font.pointSize: Type.micro
                color: Theme.color.textSecondary
            }
        }
    }
}
