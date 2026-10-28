// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import "../Theme"

// The software and shader-error path shares the same bounded animation state.
Item {
    id: fallback
    objectName: "compactVisualizerFallback"
    required property var modelState
    required property int mode
    required property color topColor
    required property color bottomColor
    // Cells per slot height (2 makes rectangles of half the height) and the peak mark's thickness in pixels.
    property real rowDivisions: 1
    property real peakThickness: 1
    readonly property real pitch: fallback.modelState.slot / Math.max(1, rowDivisions)
    Gradient {
        id: tint
        GradientStop { position: 0; color: fallback.topColor }
        GradientStop { position: 1; color: fallback.bottomColor }
    }
    Repeater {
        model: fallback.modelState.count
        delegate: Item {
            id: column
            required property int index
            x: index * fallback.modelState.slot
            width: fallback.modelState.slot
            height: fallback.modelState.height
            Rectangle {
                x: fallback.mode === 2 ? 3 : 1
                width: Math.max(0, column.width - x * 2) // derived: paired gaps
                height: Math.max(Metrics.hairline * 2, fallback.modelState.at(column.index) * column.height) // derived: two-pixel minimum
                y: fallback.mode === 1 ? (column.height - height) / 2 : column.height - height
                visible: fallback.mode < 5
                radius: 1 // intrinsic: compact column corner
                gradient: tint
            }
            Rectangle {
                visible: fallback.mode === 5 && fallback.modelState.peak(column.index) > 0.001
                x: 1
                y: Math.max(0, column.height * (1 - fallback.modelState.peak(column.index)) - height)
                width: Math.max(0, column.width - 2) // derived: paired pixel gaps
                height: Math.max(1, fallback.peakThickness)
                color: fallback.topColor
            }
            Repeater {
                model: fallback.mode >= 5 ? Math.max(1, Math.floor(column.height / fallback.pitch)) : 0
                delegate: Rectangle {
                    required property int index
                    x: 1
                    y: column.height - (index + 1) * fallback.pitch
                    width: Math.max(0, fallback.modelState.slot - 2) // derived: paired pixel gaps
                    height: Math.max(0, fallback.pitch - 2) // derived: paired pixel gaps
                    visible: (index + 0.5) * fallback.pitch <= fallback.modelState.at(column.index) * column.height
                    color: fallback.topColor
                }
            }
        }
    }
}
