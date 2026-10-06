// hot-path: shown in the row that plays.
import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// Three bars in the accent (design/03 PlayingIndicator). Still, for now:
// an animation per playing row is a cost to measure first.
Item {
    width: Metrics.playingBox
    height: Metrics.playingBox

    Row {
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Metrics.gap2

        Repeater {
            model: [0.67, 0.92, 0.5]

            Rectangle {
                required property real modelData
                anchors.bottom: parent.bottom
                width: Metrics.playingBarWidth
                height: Metrics.playingBox * modelData
                radius: 1
                color: Theme.color.accent
            }
        }
    }
}
