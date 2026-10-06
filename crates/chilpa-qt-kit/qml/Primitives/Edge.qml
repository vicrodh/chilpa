import QtQuick
import "../Theme"

// The design's hairline border, drawn over its parent's fill.
//
// A Rectangle's own border replaces the fill under it: with a translucent
// hairline, what shows through is the window behind, and on a surface the
// border vanishes into it (measured: 4 levels above the fill instead of the
// 17 of the design). Drawn as a separate outline on top, it lightens or
// darkens the surface itself, the way the design's CSS borders do.
Rectangle {
    anchors.fill: parent
    radius: parent && parent.radius !== undefined ? parent.radius : 0
    color: "transparent"
    border.color: Theme.color.strokeHairline
}
