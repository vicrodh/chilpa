import QtQuick
import QtQuick.Window
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// The window controls of the header (03-immersive §1.2): a 44 px disc
// with a sliders glyph that grows leftward to 40 plus 36 per button in
// 150 ms while the pointer is on it or it is pinned by a click: minimize,
// maximize or restore, full screen, and the X that leaves the view. The
// buttons gate on the state, never on the width in motion.
Rectangle {
    id: capsule
    property Window hostWindow: null
    signal exit()
    readonly property bool platformButtons: Qt.platform.os !== "osx"
    readonly property int slots: platformButtons ? 4 : 2
    property bool pinned: false
    readonly property bool anyHover: body.containsMouse || minimize.hovered || maximize.hovered || fullscreen.hovered || exitButton.hovered
    readonly property bool expanded: pinned || anyHover
    // Its width open, whatever it is now.
    readonly property int openWide: Stage.capsuleBase + Stage.capsuleStep * slots
    readonly property bool fullScreen: hostWindow !== null && hostWindow.visibility === Window.FullScreen
    readonly property bool maximized: hostWindow !== null && hostWindow.visibility === Window.Maximized

    width: expanded ? openWide : Stage.capsuleIdle
    height: Stage.headerTall
    radius: Stage.capsuleRound
    color: Theme.color.immersiveGlass
    border.width: Metrics.hairline
    border.color: Theme.color.immersiveGlassEdge
    clip: true
    Behavior on width { NumberAnimation { duration: Motion.reduced ? 0 : Stage.capsuleSlide; easing.type: Easing.InOutQuad } }

    MouseArea {
        id: body
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        // The disc pins it open; a click on the open body unpins, and the
        // hover keeps it open until the pointer leaves.
        onClicked: capsule.pinned = !capsule.pinned
    }
    Text {
        anchors.centerIn: parent
        text: Icons.adjustmentsHorizontal
        font.family: Icons.outline
        font.pixelSize: Stage.capsuleIcon
        color: Theme.color.immersiveTextStrong
        opacity: capsule.expanded ? 0 : 1
        Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.glyphFade } }
    }

    component CapsuleAction: Item {
        id: action
        property string glyph
        property int fromRight
        property bool shown: true
        readonly property bool hovered: area.containsMouse
        signal clicked()
        visible: shown
        x: capsule.width - fromRight
        width: Stage.capsuleIcon
        height: Stage.capsuleIcon
        anchors.verticalCenter: parent.verticalCenter
        opacity: capsule.expanded && shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Motion.reduced ? 0 : Stage.glyphFade } }
        Text {
            anchors.centerIn: parent
            text: action.glyph
            font.family: Icons.outline
            font.pixelSize: Stage.capsuleIcon - 1 // derived: the action glyphs sit a pixel under the disc's
            color: area.containsMouse ? Theme.color.immersiveTextStrong : Theme.color.immersiveTextMuted
        }
        MouseArea {
            id: area
            anchors.fill: parent
            anchors.margins: -Stage.capsuleHitPad
            enabled: capsule.expanded && action.shown
            hoverEnabled: enabled
            cursorShape: Qt.PointingHandCursor
            onClicked: action.clicked()
        }
    }
    CapsuleAction {
        id: minimize
        glyph: Icons.minus
        shown: capsule.platformButtons
        fromRight: Stage.capsuleStep * 4 // derived: the fourth slot from the right
        onClicked: if (capsule.hostWindow) capsule.hostWindow.showMinimized()
    }
    CapsuleAction {
        id: maximize
        glyph: capsule.maximized ? Icons.copy : Icons.square
        shown: capsule.platformButtons
        fromRight: Stage.capsuleStep * 3 // derived: the third slot from the right
        onClicked: if (capsule.hostWindow) capsule.hostWindow.visibility = capsule.maximized ? Window.Windowed : Window.Maximized
    }
    CapsuleAction {
        id: fullscreen
        glyph: capsule.fullScreen ? Icons.browserMaximize : Icons.arrowsMaximize
        fromRight: Stage.capsuleStep * 2 // derived: the second slot from the right
        onClicked: if (capsule.hostWindow) capsule.hostWindow.visibility = capsule.fullScreen ? Window.Windowed : Window.FullScreen
    }
    CapsuleAction {
        id: exitButton
        glyph: Icons.x
        fromRight: Stage.capsuleStep
        onClicked: capsule.exit()
    }
}
