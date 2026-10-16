import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A transient, readable home for problems reported by the engine. Playback
// errors must never exist only in a log: a silent track needs an explanation.
Item {
    id: toast

    property string message
    property bool warning: false

    width: Math.min(Metrics.toastMaxWidth, Math.max(Metrics.toastMinWidth, label.implicitWidth + Metrics.gap16 * 2)) // derived: padding on both sides
    height: label.implicitHeight + Metrics.gap16
    visible: false
    opacity: visible ? 1 : 0

    Behavior on opacity { NumberAnimation { duration: Motion.fast } }

    RectangularShadow {
        anchors.fill: card
        radius: card.radius
        offset.y: Metrics.shadowKeyY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }

    Rectangle {
        id: card
        anchors.fill: parent
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised

        Rectangle {
            anchors.left: parent.left
            width: Metrics.gap4
            height: parent.height
            radius: parent.radius
            color: toast.warning ? Theme.color.statusWarning : Theme.color.statusNegative
        }

        Edge {}

        Text {
            id: label
            anchors.fill: parent
            anchors.leftMargin: Metrics.gap16
            anchors.rightMargin: Metrics.gap16
            verticalAlignment: Text.AlignVCenter
            text: toast.message
            wrapMode: Text.Wrap
            font.pointSize: Type.body
            color: Theme.color.textPrimary
        }
    }

    Timer {
        id: hideTimer
        interval: Motion.toast
        onTriggered: toast.visible = false
    }

    function show(text, warning) {
        toast.message = text
        toast.warning = warning
        toast.visible = true
        hideTimer.restart()
    }

    Connections {
        target: Player

        function onErrorChanged() {
            if (Player.error !== "")
                toast.show(Player.error, false)
        }
    }

    // What the startup did to the graphics: a GPU that fell to Auto, a
    // renderer that fell to software.
    Component.onCompleted: { const said = Graphics.takeNotices(); if (said !== "") show(said, true) }

}
