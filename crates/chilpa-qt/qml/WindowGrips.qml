import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The edges of a frameless window: dragging one hands the resize to the
// window system, which keeps its own snapping and limits.
Item {
    id: grips

    required property Window window
    property int size: Metrics.windowGrip

    Repeater {
        model: [
            { edges: Qt.LeftEdge, x: 0, y: 0, w: 0, h: 1, cursor: Qt.SizeHorCursor },
            { edges: Qt.RightEdge, x: 1, y: 0, w: 0, h: 1, cursor: Qt.SizeHorCursor },
            { edges: Qt.TopEdge, x: 0, y: 0, w: 1, h: 0, cursor: Qt.SizeVerCursor },
            { edges: Qt.BottomEdge, x: 0, y: 1, w: 1, h: 0, cursor: Qt.SizeVerCursor },
            { edges: Qt.LeftEdge | Qt.BottomEdge, x: 0, y: 1, w: 0, h: 0, cursor: Qt.SizeBDiagCursor },
            { edges: Qt.RightEdge | Qt.BottomEdge, x: 1, y: 1, w: 0, h: 0, cursor: Qt.SizeFDiagCursor }
        ]

        MouseArea {
            required property var modelData
            x: modelData.x * (grips.width - grips.size)
            y: modelData.y * (grips.height - grips.size)
            width: modelData.w * grips.width + (1 - modelData.w) * grips.size
            height: modelData.h * grips.height + (1 - modelData.h) * grips.size
            cursorShape: modelData.cursor
            enabled: grips.window.visibility !== Window.Maximized
            onPressed: grips.window.startSystemResize(modelData.edges)
        }
    }
}
