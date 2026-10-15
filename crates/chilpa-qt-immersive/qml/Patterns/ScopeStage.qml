import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/ambient/qml/Components" as Fields
import "../Components"
import "../Theme"

Item {
    id: stage
    property int scopeMode: 0
    property url atmosphere
    Fields.AtmosphereField {
        anchors.fill: parent
        source: stage.atmosphere
        fallbackSource: Player.art
        animated: false
        opacity: 0.78
        dim: Stage.atmosphereDim
    }
    Rectangle { anchors.fill: parent; color: Theme.color.immersiveScrim }
    ScopePanel {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Stage.scopeBottom
        width: stage.scopeMode === 0 ? Math.min(stage.width - Stage.scopeSide * 2, stage.height - Stage.scopeHeightLess) : stage.width - Stage.scopeSide * 2 // derived: paired side margins
        height: stage.scopeMode === 0 ? width : stage.height - Stage.scopeRightTall - Stage.scopeBottom
        scopeMode: stage.scopeMode
    }
}
