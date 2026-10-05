import QtQuick
import "../Theme"
import "../Primitives"

SourceIcon {
    property TrackColumns cols
    property string name
    x: cols.sourceX + (cols.sourceWidth - width) / 2 // derived: centered in the source column
    width: Metrics.iconSm
    height: Metrics.iconSm
    visible: cols.sourceWidth > 0
}
