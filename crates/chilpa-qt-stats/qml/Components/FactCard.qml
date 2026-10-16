import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One fact of the period: its icon, its value large and what it is.
Rectangle {
    id: fact

    property string glyph
    property string value
    property string label
    property color tint: Theme.color.accent

    radius: Metrics.radiusPanel
    color: Theme.ambient ? Theme.color.ambientPanel : Theme.color.surfacePanel
    border.width: 1
    border.color: Theme.color.strokeHairline

    Icon {
        id: mark
        x: Metrics.gap16
        y: Metrics.gap16
        glyph: fact.glyph
        size: Metrics.iconMd
        color: fact.tint
    }
    Column {
        anchors.left: mark.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap16
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: fact.value
            elide: Text.ElideRight
            font.pointSize: Charts.factNumber
            font.weight: Type.bold
            font.features: { "tnum": 1 }
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: fact.label
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
}
