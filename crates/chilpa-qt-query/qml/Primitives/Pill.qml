import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// A preset to pick (F3): 26 tall, radius 13; the chosen one is inverted.
Rectangle {
    id: pill
    property string text
    property bool on: false
    signal clicked()
    implicitWidth: label.implicitWidth + Refine.pillPad * 2 // derived: padding on both sides
    implicitHeight: Refine.pillTall
    radius: Refine.pillRound
    color: on ? Theme.color.textPrimary : Theme.color.fillField
    Text {
        id: label
        anchors.centerIn: parent
        text: pill.text
        font.pointSize: Type.row
        font.weight: Type.medium
        font.features: { "tnum": 1 }
        color: pill.on ? Theme.color.surfaceWindow : Theme.color.textPrimary
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: pill.clicked()
    }
}
