import QtQuick
import "../Theme"

Item {
    id: overlay
    required property var pointer
    required property var rows
    required property TrackColumns cols
    Rectangle {
        objectName: "dropLine"
        readonly property Item target: overlay.pointer.dropRow >= 0 && overlay.pointer.dropRow !== overlay.pointer.pressedRow ? overlay.rows.itemAtIndex(overlay.pointer.dropRow) : null
        visible: !!target && !target.header
        x: overlay.cols.pad
        y: overlay.rows.y + (target ? target.y - overlay.rows.contentY + (overlay.pointer.dropAfter ? target.height : 0) : 0) - height / 2
        z: 4
        width: overlay.width - overlay.cols.pad * 2 // derived: padding on both sides
        height: Metrics.gap2
        radius: Metrics.gap2
        color: Theme.color.accent
    }
    DragGhost {
        objectName: "dragGhost"
        visible: overlay.pointer.dragging
        source: overlay.pointer
        at: Qt.point(overlay.pointer.mouseX, overlay.pointer.mouseY)
        count: overlay.pointer.dragCount
        title: overlay.pointer.dragTitle
        detail: overlay.pointer.dragDetail
    }
}
