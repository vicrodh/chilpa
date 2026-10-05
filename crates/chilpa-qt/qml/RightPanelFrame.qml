import QtQuick
import QtQuick.Effects
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/sources/qml/Patterns"
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
        mediaContent: MediaListeningPanel { document: JSON.parse(Sources.snapshot || "{}"); onClosed: SettingsModel.lyricsOpen = false }
        anchors.fill: parent
    }
}
