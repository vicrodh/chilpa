// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
import QtQuick
import app.chilpa.graphics
import "../Theme"

// I expose only the compact renderer contract to the shell composition.
Item {
    id: scope
    property int mode: 0
    property var points: []
    property color traceColor
    ScopeTraceItem {
        anchors.fill: parent
        anchors.margins: Stage.compactScopeInset
        mode: scope.mode
        points: scope.points
        traceColor: scope.traceColor
        lineWidth: Stage.compactScopeLine
        trailDepth: scope.mode === 0 ? 3 : 2
        fillOpacity: Stage.compactScopeFill
    }
}
