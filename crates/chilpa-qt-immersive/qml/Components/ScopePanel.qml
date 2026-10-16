// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import app.chilpa.ambient
import app.chilpa.graphics
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

Item {
    id: panel
    property int scopeMode: 0
    property var feed: Visualizer
    property color traceColor: Ambient.brightest
    readonly property real guideOpacity: scopeMode === 0 ? 0.19 : 0.12

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.08
        width: Metrics.hairline
        height: parent.height * 0.84
        color: panel.traceColor
        opacity: panel.guideOpacity
    }
    Rectangle {
        x: parent.width * 0.08
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width * 0.84
        height: Metrics.hairline
        color: panel.traceColor
        opacity: panel.guideOpacity
    }
    Repeater {
        model: panel.scopeMode === 1 ? 3 : 0
        Rectangle {
            required property int index
            x: parent.width * (index + 1) / 4
            y: parent.height * 0.14
            width: Metrics.hairline
            height: parent.height * 0.72
            color: panel.traceColor
            opacity: 0.07
        }
    }
    Repeater {
        model: panel.scopeMode === 0 ? [0.84, 0.46] : []
        Rectangle {
            required property real modelData
            required property int index
            anchors.centerIn: parent
            width: parent.width * modelData
            height: width
            color: "transparent"
            border.width: Metrics.hairline
            border.color: panel.traceColor
            opacity: index === 0 ? 0.18 : 0.09
            radius: width / 2 // derived: circular guide
        }
    }
    Repeater {
        model: panel.scopeMode === 0 ? 2 : 0
        Rectangle {
            required property int index
            anchors.centerIn: parent
            width: parent.width * 0.76
            height: Metrics.hairline
            rotation: index === 0 ? 45 : -45
            color: panel.traceColor
            opacity: 0.08
        }
    }
    ScopeTraceItem {
        objectName: "immersiveScopeTrace"
        anchors.fill: parent
        anchors.margins: Stage.scopeTracePad
        mode: panel.scopeMode
        points: panel.scopeMode === 0 ? panel.feed.goniometer : panel.feed.oscilloscope
        traceColor: panel.traceColor
        lineWidth: Stage.scopeLine
        trailDepth: panel.scopeMode === 0 ? 5 : 4
        fillOpacity: Stage.scopeFill
    }
    Item {
        visible: panel.scopeMode === 0
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Stage.scopeCorrelationEdge
        width: Stage.scopeCorrelationWide
        height: Stage.scopeCorrelationTall
        Rectangle {
            anchors.left: parent.left
            anchors.right: value.left
            anchors.rightMargin: Stage.scopeTracePad
            anchors.verticalCenter: parent.verticalCenter
            height: Stage.scopeCorrelationTrack
            radius: height / 2 // derived: round track ends
            color: panel.traceColor
            opacity: 0.22
            Rectangle {
                width: Stage.scopeCorrelationThumbWide
                height: Stage.scopeCorrelationThumbTall
                radius: width / 2 // derived: round thumb ends
                anchors.verticalCenter: parent.verticalCenter
                x: (parent.width - width) * Math.max(0, Math.min(1, (panel.feed.stereoCorrelation + 1) * 0.5))
                color: panel.traceColor
            }
        }
        Text {
            id: value
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: panel.feed.stereoCorrelation.toFixed(2)
            color: panel.traceColor
            opacity: 0.72
            font.pointSize: Stage.spectralLabel
        }
    }
}
