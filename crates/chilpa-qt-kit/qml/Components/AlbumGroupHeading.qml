import QtQuick
import "../Theme"

// A plain section label above grouped album rows.
Text {
    height: Metrics.rowGroup
    verticalAlignment: Text.AlignVCenter
    elide: Text.ElideRight
    font.pointSize: Type.meta
    font.weight: Type.semibold
    color: Theme.color.textSecondary
}
