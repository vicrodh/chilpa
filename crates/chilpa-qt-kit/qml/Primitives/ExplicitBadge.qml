import QtQuick
import "../Theme"

Rectangle {
    objectName: "explicitBadge"
    implicitWidth: Metrics.iconXs + Metrics.gap2
    implicitHeight: implicitWidth
    radius: Metrics.radiusArtSmall
    color: Theme.color.fillField
    Accessible.role: Accessible.StaticText
    Accessible.name: Theme.explicitLabel
    Text {
        anchors.centerIn: parent
        text: "E"
        textFormat: Text.PlainText
        font.pointSize: Type.micro
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }
}
