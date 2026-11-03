import QtQuick
import "../Theme"

// The title's existing loader creates this row only when a label is present.
// No clock, animation or provider lookup belongs to a content advisory.
Row {
    property bool explicitContent: false
    property bool aiGenerated: false
    spacing: Metrics.gap2
    Loader {
        active: parent.explicitContent
        sourceComponent: ExplicitBadge {}
    }
    Loader {
        active: parent.aiGenerated
        sourceComponent: AiBadge {}
    }
}
