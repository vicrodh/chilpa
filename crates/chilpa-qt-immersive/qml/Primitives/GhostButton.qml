import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A quiet round button of the immersive bar (03-immersive §1.3): a 28 px
// circle with no fill until it is on, a small white glyph at 65 % that
// brightens on hover, and a dimmed whole when it cannot be used. Fixed
// white alphas on purpose: the bar is dark under every theme.
Rectangle {
    id: ghost
    property string glyph
    property bool filled: false
    property bool active: false
    property bool able: true
    property bool busy: false
    property QtObject clock: null
    property bool animationActive: true
    signal clicked()
    width: Stage.ghostSide
    height: Stage.ghostSide
    radius: width / 2 // derived: a circle
    opacity: able ? 1 : Stage.disabledOpacity
    color: active ? Theme.color.immersiveFill : "transparent"
    Text {
        visible: !ghost.busy
        anchors.centerIn: parent
        text: ghost.glyph
        font.family: ghost.filled ? Icons.filled : Icons.outline
        font.pixelSize: Stage.ghostIcon
        color: Theme.color.immersiveTextStrong
        opacity: ghost.active || area.containsMouse ? 1 : Stage.idleGlyph
    }
    Loader {
        anchors.centerIn: parent
        active: ghost.busy && ghost.visible && ghost.animationActive
        sourceComponent: BusyMark { clock: ghost.clock; size: Stage.ghostIcon; color: Theme.color.immersiveTextStrong }
    }
    MouseArea {
        id: area
        anchors.fill: parent
        enabled: ghost.able
        hoverEnabled: true
        cursorShape: ghost.able ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (!ghost.busy) ghost.clicked()
    }
}
