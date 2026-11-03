import QtQuick
import "../Theme"

Rectangle {
    objectName: "aiBadge"
    implicitWidth: Math.max(implicitHeight, label.implicitWidth + Metrics.gap4)
    implicitHeight: Metrics.iconXs + Metrics.gap2
    radius: Metrics.radiusArtSmall
    color: Theme.color.fillField
    Accessible.role: Accessible.StaticText
    Accessible.name: Theme.aiGeneratedLabel
    Text {
        id: label
        anchors.centerIn: parent
        text: "AI"
        textFormat: Text.PlainText
        font.pointSize: Type.micro
        font.weight: Type.semibold
        color: Theme.color.textSecondary
    }
}
