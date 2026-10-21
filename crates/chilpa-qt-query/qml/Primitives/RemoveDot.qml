import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The small x that takes one thing away: 18 square, round on a chip
// (radius 9), squarer inside a joined block of values (radius 5).
Item {
    id: dot
    required property string tip
    property int round: Refine.chipRemoveRound
    property color tint: Theme.color.textSecondary
    signal clicked()
    implicitWidth: Refine.valueRemove
    implicitHeight: Refine.valueRemove

    Rectangle {
        anchors.fill: parent
        radius: dot.round
        color: area.containsMouse ? Theme.color.stateSelected : "transparent"
    }
    Icon {
        anchors.centerIn: parent
        glyph: Icons.x
        size: Refine.valueRemoveIcon
        color: dot.tint
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: dot.clicked()
        onContainsMouseChanged: containsMouse ? TipState.show(dot.tip, dot) : TipState.hide(dot)
    }
    Component.onDestruction: TipState.hide(dot)
}
