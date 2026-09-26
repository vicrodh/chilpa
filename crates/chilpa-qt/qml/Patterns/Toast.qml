import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// A transient, readable home for problems reported by the engine. Playback
// errors must never exist only in a log: a silent track needs an explanation.
Item {
    id: toast

    property string message

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
            color: Theme.color.statusNegative
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

    Connections {
        target: Player

        function onErrorChanged() {
            if (Player.error === "")
                return
            toast.message = Player.error
            toast.visible = true
            hideTimer.restart()
        }
    }
}
