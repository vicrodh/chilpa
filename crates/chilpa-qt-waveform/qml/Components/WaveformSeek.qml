import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.waveform
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// The seek bar as a waveform: a bar per pixel from the track's downward
// peak to its upward one there, over a rail, dense as Nulloy's; the bars
// already played in the accent, the ones the document covers in the
// secondary text color, the rest as hairlines. It seeks as the thin bar does: a click goes there, a
// drag follows the pointer and goes where it is let go, with the time
// under the pointer in a bubble, and the A-B loop's marks over it. The
// values come from the bridge for the columns this width has room for.
Item {
    id: bar

    property real position: 0
    property real duration: 0
    property string bubbleText
    property real loopStart: -1
    property real loopEnd: -1

    signal seek(real seconds)

    readonly property bool active: area.containsMouse || area.pressed
    readonly property real shown: area.pressed ? dragged : position
    readonly property real hoverSeconds: secondsAt(area.mouseX)
    property real dragged: 0
    function secondsAt(x) { return duration * Math.max(0, Math.min(1, x / width)) }

    readonly property int columns: Math.max(Bars.columnsMin, Math.min(Bars.columnsMax, Math.ceil(width / Bars.pixelsPerColumn)))
    readonly property var values: Waveform.values
    readonly property var lows: Waveform.lows
    readonly property color shade: Theme.ambient ? Theme.color.ambientPlayer : Theme.color.surfacePlayer
    readonly property real played: duration > 0 ? Math.min(1, shown / duration) : 0
    readonly property real slot: width / Math.max(1, columns)
    readonly property real barWide: Math.max(Metrics.hairline, Math.floor(slot))
    readonly property real halfTall: Math.max(1, height * Bars.halfShare)

    onColumnsChanged: Waveform.columns = columns
    Component.onCompleted: { Waveform.columns = columns; Waveform.refresh() }
    onVisibleChanged: if (visible) Waveform.refresh()
    // While the document grows it is read ten times a second; complete, it
    // is read once per track.
    Timer { interval: Bars.refreshMs; repeat: true; running: bar.visible && !Waveform.complete && Player.playing; onTriggered: Waveform.refresh() }
    Connections { target: Player; function onPathChanged() { Waveform.refresh() } }

    Rectangle {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        height: Bars.railTall
        color: Theme.color.strokeHairline
        opacity: Bars.railOpacity
    }
    Repeater {
        id: bars
        model: bar.values.length === bar.columns && bar.lows.length === bar.columns ? bar.columns : 0
        Rectangle {
            id: column
            required property int index
            objectName: "waveformBar"
            readonly property real value: Math.min(1, Math.max(0, bar.values[index]))
            readonly property real low: Math.min(1, Math.max(0, bar.lows[index]))
            readonly property real reach: Math.min(bar.halfTall, Bars.barLeast / 2 + value * Math.max(0, bar.halfTall - Bars.barLeast / 2)) // derived: the least bar and the rest by the value
            readonly property real drop: Math.min(bar.halfTall, Bars.barLeast / 2 + low * Math.max(0, bar.halfTall - Bars.barLeast / 2)) // derived: the same, downward
            readonly property bool sung: (index + 1) / bar.columns <= bar.played
            readonly property bool known: index / bar.columns < Waveform.analyzed
            x: bar.slot * index
            y: bar.height / 2 - reach // derived: from the rail up to the upward peak
            width: bar.barWide
            height: reach + drop // derived: the upward peak and the downward one
            visible: value > 0.0005 || low > 0.0005
            color: sung ? Theme.color.accent : (known ? Theme.color.textSecondary : Theme.color.strokeHairline)
        }
    }
    // The shade: the surface fading out toward the rail, over the bars.
    Rectangle {
        anchors.fill: parent
        opacity: Bars.shadeOpacity
        gradient: Gradient {
            GradientStop { position: 0.0; color: bar.shade }
            GradientStop { position: 0.5; color: "transparent" }
            GradientStop { position: 1.0; color: bar.shade }
        }
    }
    // The loop's stretch once it has an end, and a tick at each mark.
    Rectangle {
        visible: bar.loopEnd > bar.loopStart && bar.duration > 0
        x: bar.width * bar.loopStart / Math.max(bar.duration, 1)
        width: Math.max(Metrics.gap2, bar.width * (bar.loopEnd - bar.loopStart) / Math.max(bar.duration, 1))
        height: parent.height
        color: Theme.color.accent
        opacity: Metrics.notchSoft
    }
    Repeater {
        model: bar.duration > 0 ? [bar.loopStart, bar.loopEnd].filter(at => at >= 0) : []
        Rectangle {
            required property real modelData
            objectName: "loopMark"
            x: bar.width * modelData / bar.duration - width / 2 // derived: centered on the mark
            width: Metrics.gap2
            height: parent.height
            color: Theme.color.accent
        }
    }
    // The cursor: a hairline over the bars at the position.
    Rectangle {
        objectName: "waveformCursor"
        visible: bar.duration > 0
        x: bar.width * bar.played - width / 2 // derived: centered on the position
        width: Bars.cursorWide
        height: parent.height
        color: Theme.color.accent
        opacity: bar.active ? 1 : Bars.cursorRest
    }

    Rectangle {
        visible: bar.active && bar.duration > 0 && bar.bubbleText !== ""
        width: label.implicitWidth + Metrics.gap10
        height: label.implicitHeight + Metrics.gap4
        radius: Metrics.radiusItem
        color: Theme.color.surfaceRaised
        x: Math.max(0, Math.min(bar.width - width, area.mouseX - width / 2)) // derived: over the pointer, inside the bar
        anchors.bottom: parent.top
        anchors.bottomMargin: Metrics.gap2

        Edge { radius: parent.radius }

        Text {
            id: label
            anchors.centerIn: parent
            text: bar.bubbleText
            font.pointSize: Type.micro
            font.features: { "tnum": 1 }
            color: Theme.color.textPrimary
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        enabled: bar.duration > 0
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        preventStealing: true
        onPressed: (mouse) => bar.dragged = bar.secondsAt(mouse.x)
        onPositionChanged: (mouse) => { if (pressed) bar.dragged = bar.secondsAt(mouse.x) }
        onReleased: (mouse) => bar.seek(bar.dragged)
    }
}
