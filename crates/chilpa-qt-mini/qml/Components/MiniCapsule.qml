import QtQuick
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// The hover capsule (02-mini §1.5, QBZ's MiniWindowControls): a disc at
// the right that unfolds LEFT into ten buttons, five surface tabs, a
// separator, then expand, pin, background, move and close. Inline chrome,
// never a popup: a popup would take the focus from a frameless window.
// Its width is spelled out from constants, not read off the children. One
// hover sensor on the chrome opens it and arms a short grace before it
// folds; the pointer leaving the whole card folds it at once. The slide is
// the only animated geometry in the mini player. The pin exists only where
// the desktop honours it (Windows); elsewhere its place stays empty, as in
// QBZ, and keeping a window above others is the compositor's rule.
Rectangle {
    id: capsule
    objectName: "miniWindowControls"

    // The lit tab; passed in so the mount owns the binding.
    property int activeSurface: 2
    // The mini player's window, for the move button.
    property var hostWindow: null
    property bool windowHovered: true
    property bool expanded: false
    readonly property bool pinnable: Qt.platform.os === "windows"
    property real slotWidth: expanded ? Card.capsuleCluster : 0
    Behavior on slotWidth {
        NumberAnimation {
            duration: Motion.reduced ? 0 : Card.capsuleSlide
            easing.type: Easing.Bezier
            easing.bezierCurve: [0.4, 0, 0.2, 1, 1, 1]
        }
    }

    onWindowHoveredChanged: {
        if (!windowHovered) {
            grace.stop()
            expanded = false
        }
    }

    implicitWidth: Card.capsulePad + slotWidth + (expanded ? Card.capsuleApart : 0) + Card.capsuleButton + Card.capsulePad
    implicitHeight: Card.capsuleTall
    radius: Card.capsuleRound
    antialiasing: true
    color: Theme.color.surfaceRaised
    border.width: Card.edge
    border.color: Theme.color.miniCapsuleEdge

    HoverHandler {
        onHoveredChanged: {
            if (hovered) {
                grace.stop()
                capsule.expanded = true
            } else {
                grace.restart()
            }
        }
    }
    Timer {
        id: grace
        interval: Card.capsuleGrace
        onTriggered: capsule.expanded = false
    }

    // The cluster slides out of a shrinking box rather than scaling.
    Item {
        id: slot
        x: Card.capsulePad
        y: Math.round((capsule.height - height) / 2) // derived: centered
        width: capsule.slotWidth
        height: Card.capsuleButton
        clip: true
        opacity: capsule.expanded ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Card.capsuleFade } }

        Row {
            height: Card.capsuleButton
            spacing: Card.capsuleApart

            Row {
                height: Card.capsuleButton
                spacing: Card.capsuleBetween
                Repeater {
                    model: [Icons.minus, Icons.disc, Icons.photo, Icons.playlist, Icons.quote]
                    CapsuleButton {
                        required property int index
                        required property string modelData
                        anchors.verticalCenter: parent.verticalCenter
                        glyph: modelData
                        active: capsule.activeSurface === index
                        onClicked: Mini.surface = index
                    }
                }
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: Card.edge
                height: Card.separatorTall
                color: Theme.color.miniSeparator
            }
            Row {
                height: Card.capsuleButton
                spacing: Card.capsuleBetween
                CapsuleButton {
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.arrowsMaximize
                    onClicked: Mini.exit()
                }
                CapsuleButton {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: capsule.pinnable
                    glyph: Icons.pin
                    active: capsule.hostWindow ? capsule.hostWindow.pinnedOnTop : false
                    onClicked: if (capsule.hostWindow) capsule.hostWindow.pinnedOnTop = !capsule.hostWindow.pinnedOnTop
                }
                CapsuleButton {
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.stack2
                    active: Mini.backgroundBlur
                    onClicked: Mini.toggleBackgroundBlur()
                }
                CapsuleButton {
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.arrowsMove
                    iconSize: Card.capsuleMoveIcon
                    dragCursor: true
                    onPressed: if (capsule.hostWindow) capsule.hostWindow.startSystemMove()
                }
                CapsuleButton {
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.x
                    danger: true
                    onClicked: Mini.closeApp()
                }
            }
        }
    }

    // The trigger: square, so the folded capsule is a disc around it.
    CapsuleButton {
        objectName: "miniMenuTrigger"
        glyph: Icons.square
        idleOpacity: 1
        iconSize: Card.capsuleMoveIcon
        width: Card.capsuleButton
        height: Card.capsuleButton
        x: capsule.width - Card.capsulePad - Card.capsuleButton
        y: Math.round((capsule.height - height) / 2) // derived: centered
        onClicked: {
            grace.stop()
            capsule.expanded = true
        }
    }
}
