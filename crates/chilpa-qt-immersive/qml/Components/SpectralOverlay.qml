// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import app.chilpa
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

Item {
    id: overlay
    property var feed: Visualizer
    property bool headerVisible: true
    readonly property real plotX0: width * 0.055
    readonly property real plotX1: width * 0.970
    readonly property real plotY0: height * 0.070
    readonly property real plotY1: height * 0.780
    readonly property real plotWide: plotX1 - plotX0
    readonly property real plotTall: plotY1 - plotY0
    readonly property real nyquistKhz: feed.sampleRateHz > 0 ? feed.sampleRateHz / 2000 : 22.05
    function stamp(seconds) {
        return Math.floor(seconds / 60) + ":" + (seconds % 60 < 10 ? "0" : "") + Math.floor(seconds % 60)
    }
    Rectangle {
        visible: overlay.headerVisible && overlay.feed.sampleRateHz > 0
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.045
        width: header.implicitWidth + Stage.spectralHeaderPad // derived: text with paired padding
        height: Stage.spectralHeaderTall
        radius: Stage.spectralHeaderRound
        border.width: Metrics.hairline
        border.color: Theme.color.immersiveAxis
        color: Theme.color.immersiveAxisBox
        Text {
            id: header
            anchors.centerIn: parent
            text: "1/1 · " + overlay.feed.sampleRateHz + " Hz · " + overlay.feed.bitDepth + " bits · FFT:4096 · 512"
            color: Theme.color.immersiveAxis
            font.pointSize: Stage.spectralHeaderText
        }
    }
    Rectangle { x: overlay.plotX0; y: overlay.plotY0; width: Metrics.hairline; height: overlay.plotTall; color: Theme.color.immersiveAxis }
    Rectangle { x: overlay.plotX0; y: overlay.plotY1; width: overlay.plotWide; height: Metrics.hairline; color: Theme.color.immersiveAxis }
    Repeater {
        model: [0, 0.25, 0.5, 0.75, 1]
        Text {
            required property real modelData
            x: overlay.plotX0 - Stage.spectralLabelLead
            y: overlay.plotY1 - modelData * overlay.plotTall - height / 2
            width: Stage.spectralLabelWide
            horizontalAlignment: Text.AlignRight
            text: Math.round(overlay.nyquistKhz * modelData) + "k"
            color: Theme.color.immersiveAxis
            font.pointSize: Stage.spectralLabel
        }
    }
    Repeater {
        model: Math.min(Stage.spectralMaxLabels, Math.floor(Player.duration / Stage.spectralTimeStep) + 1)
        Text {
            required property int index
            x: overlay.plotX0 + (Player.duration > 0 ? overlay.plotWide * index * Stage.spectralTimeStep / Player.duration : 0) - width / 2
            y: overlay.plotY1 + Stage.spectralLabelApart
            text: overlay.stamp(index * Stage.spectralTimeStep)
            color: Theme.color.immersiveAxis
            font.pointSize: Stage.spectralTimeText
        }
    }
    Text {
        visible: Player.duration > 0
        x: overlay.plotX1 - width / 2
        y: overlay.plotY1 + Stage.spectralLabelApart
        text: overlay.stamp(Player.duration)
        color: Theme.color.immersiveAxis
        font.pointSize: Stage.spectralTimeText
    }
}
