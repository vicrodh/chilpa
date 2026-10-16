import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Side buttons resolve above exclusive pointer grabs without taking wheels.
MouseArea {
    required property var window
    parent: window ? window.contentItem : null
    anchors.fill: parent
    z: Metrics.tooltipLayer + Metrics.gap2
    acceptedButtons: Qt.BackButton | Qt.ForwardButton
    cursorShape: undefined
    onClicked: mouse => mouse.button === Qt.BackButton ? Navigation.back() : Navigation.forward()
}
