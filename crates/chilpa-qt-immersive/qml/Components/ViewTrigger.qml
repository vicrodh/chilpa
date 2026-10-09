import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The view picker of the header (03-immersive §1.2): a 122 by 36 glass
// capsule in three cells, previous | menu | next, with hairlines between
// them and a 9 % white wash under the pointer.
Rectangle {
    id: trigger
    signal previous()
    signal menu()
    signal next()
    width: Stage.triggerWide
    height: Stage.headerTall
    radius: Stage.triggerRound
    color: Theme.color.immersiveGlass
    border.width: Metrics.hairline
    border.color: Theme.color.immersiveGlassEdge
    clip: true

    Repeater {
        model: [
            { at: 0, wide: Stage.triggerArrow, glyph: Icons.chevronLeft, act: "previous" },
            { at: Stage.triggerArrow, wide: Stage.triggerMiddle, glyph: Icons.layoutGrid, act: "menu" },
            { at: Stage.triggerArrow + Stage.triggerMiddle, wide: Stage.triggerArrow, glyph: Icons.chevronRight, act: "next" }
        ]
        Rectangle {
            id: cell
            required property var modelData
            x: modelData.at
            width: modelData.wide
            height: trigger.height
            color: area.containsMouse ? Theme.color.immersiveGlassHover : "transparent"
            Text {
                anchors.centerIn: parent
                text: cell.modelData.glyph
                font.family: Icons.outline
                font.pixelSize: cell.modelData.act === "menu" ? Stage.triggerIcon : Stage.triggerIcon - 1 // derived: the arrows are a pixel smaller
                color: Theme.color.immersiveTextStrong
            }
            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    if (cell.modelData.act === "previous") trigger.previous()
                    else if (cell.modelData.act === "next") trigger.next()
                    else trigger.menu()
                }
            }
        }
    }
    Rectangle { x: Stage.triggerArrow; width: Metrics.hairline; height: parent.height; color: Theme.color.immersiveSeparator }
    Rectangle { x: Stage.triggerArrow + Stage.triggerMiddle - Metrics.hairline; width: Metrics.hairline; height: parent.height; color: Theme.color.immersiveSeparator }
}
