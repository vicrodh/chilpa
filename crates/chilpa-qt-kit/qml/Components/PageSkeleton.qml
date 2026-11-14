import QtQuick
import "../Theme"

Column {
    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
    anchors.margins: Metrics.contentX
    spacing: Metrics.gap16
    Rectangle { width:parent.width * 0.36; height:Metrics.gap16; radius:Metrics.gap8; color:Theme.color.stateHover }
    Repeater {
        model:5
        Rectangle {
            required property int index
            width:parent.width; height:Metrics.settingsRow; radius:Metrics.radiusControl
            color:Theme.color.stateHover; opacity:index % 2 ? 0.55 : 0.8
        }
    }
}
