import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "../Theme"

// A shortcut as one key cap: the whole combination in one chip, in a
// fixed-width face. In the editor it is a button, and its edge says what is
// going on: being recorded, changed from its default, or taken by another
// action.
Rectangle {
    id: cap

    property string text
    property bool button: false
    property bool recording: false
    property bool modified: false
    property bool conflict: false

    signal clicked()

    implicitWidth: Math.max(button ? Sizes.keycapMin : 0, label.implicitWidth + (button ? Metrics.gap24 : Metrics.gap16))
    implicitHeight: button ? Metrics.fieldHeightLarge : label.implicitHeight + Metrics.gap6
    radius: Metrics.radiusCard
    color: recording || pointer.hovered ? Theme.color.stateSelected : button ? Theme.color.fillField : Theme.color.surfaceWindow
    border.width: recording || modified ? Metrics.gap2 : 1
    border.color: conflict ? Theme.color.statusNegative : recording || modified ? Theme.color.accent : Theme.color.strokeHairline

    Text {
        id: label
        anchors.centerIn: parent
        text: cap.text
        font.family: "monospace"
        font.pointSize: Type.body
        font.weight: cap.button ? Type.medium : Type.regular
        color: cap.button ? Theme.color.textPrimary : Theme.color.textSecondary
    }
    HoverHandler {
        id: pointer
        enabled: cap.button
        cursorShape: Qt.PointingHandCursor
    }
    TapHandler {
        enabled: cap.button
        onTapped: cap.clicked()
    }
}
