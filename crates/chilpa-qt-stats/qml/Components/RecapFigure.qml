import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// One number of the recap: the figure, right-aligned over a short rule in
// the accent, and what it counts beside it.
Item {
    id: figure

    property string value
    property string label

    implicitHeight: number.implicitHeight + Metrics.gap4 + Charts.recapRule

    Text {
        id: number
        objectName: "recapFigureValue"
        width: Charts.recapFigure
        horizontalAlignment: Text.AlignRight
        text: figure.value
        elide: Text.ElideLeft
        font.pointSize: Charts.factNumber
        font.weight: Type.semibold
        font.features: { "tnum": 1 }
        color: Theme.color.textPrimary
    }
    Rectangle {
        y: number.implicitHeight + Metrics.gap4
        width: Charts.recapFigure
        height: Charts.recapRule
        color: Theme.color.accent
    }
    Text {
        anchors.left: number.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: parent.right
        anchors.baseline: number.baseline
        text: figure.label
        elide: Text.ElideRight
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
