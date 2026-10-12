import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The 27-key rail keeps unavailable letters visible.
Column {
    id: rail
    property var activeKeys: []
    signal jumped(string letter)
    width: Metrics.scenePadLeft
    Repeater {
        model: ["#","A","B","C","D","E","F","G","H","I","J","K","L","M","N","O","P","Q","R","S","T","U","V","W","X","Y","Z"]
        Text {
            required property string modelData
            width: rail.width; height: Metrics.sceneRailCell; horizontalAlignment: Text.AlignHCenter
            text: modelData; font.pointSize: Type.micro
            color: rail.activeKeys.includes(modelData) ? Theme.color.accent : Theme.color.textTertiary
            opacity: rail.activeKeys.includes(modelData) ? 1 : Metrics.disabledOpacity
            HoverHandler { cursorShape: rail.activeKeys.includes(modelData) ? Qt.PointingHandCursor : Qt.ArrowCursor }
            TapHandler { enabled: rail.activeKeys.includes(modelData); onTapped: rail.jumped(modelData) }
        }
    }
}
