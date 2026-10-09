import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// How a number moved against the period before, as a percent: up in the
// positive tone, down in the negative, level in the quiet one. With no
// period before to compare, it shows nothing.
Rectangle {
    id: badge

    // A whole percent, or null.
    property var change: null
    readonly property bool known: change !== null && change !== undefined
    readonly property color tone: !known || change === 0 ? Theme.color.textTertiary : change > 0 ? Theme.color.statusPositive : Theme.color.statusNegative

    visible: known
    implicitWidth: label.implicitWidth + 2 * Metrics.gap8 // derived: padding on both sides
    implicitHeight: label.implicitHeight + 2 * Metrics.gap2 // derived: padding above and below
    radius: height / 2 // derived: a pill
    color: "transparent"
    border.width: 1
    border.color: badge.tone

    Text {
        id: label
        anchors.centerIn: parent
        text: !badge.known ? "" : (badge.change > 0 ? "+" : badge.change < 0 ? "−" : "") + Math.abs(badge.change) + "%"
        font.pointSize: Type.small
        font.weight: Type.semibold
        font.features: { "tnum": 1 }
        color: badge.tone
    }
}
