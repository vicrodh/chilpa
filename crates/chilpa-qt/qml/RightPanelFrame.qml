import QtQuick
import QtQuick.Effects
import "Theme"
import "Patterns"

// The floating right panel over its shadow: the sidebar's twin, from the tab
// bar down to the window's margin.
Item {
    id: frame

    RectangularShadow {
        anchors.fill: parent
        radius: panel.radius
        offset.y: Metrics.shadowSubtleY
        blur: Metrics.shadowSubtleBlur
        color: Theme.color.shadowSubtle
    }

    RightPanel {
        id: panel
        anchors.fill: parent
    }
}
