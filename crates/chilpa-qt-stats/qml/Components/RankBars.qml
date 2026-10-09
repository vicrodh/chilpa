import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Bars that lie down, most first: a name, a bar on a faint track, and the
// count. For the genres.
Column {
    id: chart

    // [{label, value}]
    property var points: []
    property color tint: Theme.color.accent
    readonly property int most: points.reduce((m, p) => Math.max(m, p.value), 1)

    spacing: Metrics.gap8

    Repeater {
        model: chart.points

        Item {
            id: line
            required property var modelData
            width: chart.width
            height: Math.max(name.implicitHeight, Charts.rankBarTall)

            Text {
                id: name
                width: Charts.rankName
                anchors.verticalCenter: parent.verticalCenter
                text: line.modelData.label
                elide: Text.ElideRight
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            Rectangle {
                id: track
                anchors.left: name.right
                anchors.leftMargin: Metrics.gap8
                anchors.right: count.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                height: Charts.rankBarTall
                radius: height / 2 // derived: rounded ends
                color: Theme.color.stateStripe

                Rectangle {
                    width: Math.max(height, track.width * line.modelData.value / chart.most)
                    height: parent.height
                    radius: height / 2 // derived: rounded ends
                    color: chart.tint
                }
            }
            Text {
                id: count
                width: Charts.rankCount
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                text: line.modelData.value
                font.pointSize: Type.small
                font.features: { "tnum": 1 }
                color: Theme.color.textTertiary
            }
        }
    }
}
