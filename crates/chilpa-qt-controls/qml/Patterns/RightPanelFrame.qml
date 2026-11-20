import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The floating right panel over its shadow: the sidebar's twin, from the tab
// bar down to the window's margin, with a grip on its left edge. Its width
// is free in the room an open sidebar has and snaps only to its default, as
// Rust decides it; while the grip is dragged it already shows where it
// would land. It has no collapsed column: it is this wide, or closed.
Item {
    id: frame

    // Whether it is asked for: it slides in from the edge and out again,
    // the way QBZ's panels do; a drag of its grip follows the pointer
    // without the slide.
    property bool open: false
    property Component mediaContent
    visible: width > 0
    property bool dragging: false
    readonly property bool moving: dragging || slide.running
    width: open ? shown : 0
    Behavior on width {
        enabled: !frame.dragging
        NumberAnimation { id: slide; duration: Motion.networkSlide; easing.type: Easing.InOutQuad }
    }
    property real dragWidth: 0
    property real grabbed: 0
    property int kept: Number(SettingsModel.kept("ui.rightPanel.width")) || Metrics.rightPanel
    readonly property int settled: SettingsModel.panelWidthAt(kept, Window.width)
    readonly property int shown: dragging ? SettingsModel.panelWidthAt(dragWidth, Window.width) : settled

    property bool queueShown: false
    property bool lyricsShown: false
    function updateSections() {
        if (!open || (!SettingsModel.queueOpen && !SettingsModel.lyricsOpen)) return
        queueShown = SettingsModel.queueOpen
        lyricsShown = SettingsModel.lyricsOpen
    }
    onOpenChanged: updateSections()
    Component.onCompleted: updateSections()
    Connections {
        target: SettingsModel
        function onQueueOpenChanged() { frame.updateSections() }
        function onLyricsOpenChanged() { frame.updateSections() }
    }

    Shadow {
        visible: !frame.moving
        anchors.fill: parent
        radius: panel.radius
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowSubtle
    }

    Item {
        id: viewport
        anchors.fill: parent
        clip: frame.moving

        Rectangle {
            anchors.fill: parent
            visible: frame.dragging && viewport.width > panel.width
            radius: panel.radius
            color: panel.color
        }

        RightPanel {
            id: panel
            objectName: "rightPanelContent"
            queueShown: frame.queueShown
            lyricsShown: frame.lyricsShown
            mediaContent: frame.mediaContent
            width: frame.settled
            height: parent.height
        }
    }

    ResizeHandle {
        objectName: "rightPanelGrip"
        x: -width / 2
        height: parent.height
        visible: frame.open
        onStarted: {
            frame.grabbed = frame.width
            frame.dragWidth = frame.shown
            frame.dragging = true
        }
        onMoved: (delta) => frame.dragWidth = frame.grabbed - delta
        onReleased: {
            if (frame.dragging && frame.shown !== frame.settled) {
                frame.kept = frame.shown
                SettingsModel.keep("ui.rightPanel.width", String(frame.kept))
            }
            frame.dragging = false
        }
        onCanceled: frame.dragging = false
    }
}
