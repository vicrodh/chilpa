import QtQuick
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

// What draws this window, as the probe reports it: the Gallery's own window
// is watched like the main one.
Item {
    id: gallery
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the samples and their margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.gap8
        Text { text: "Renderer: " + Graphics.api; color: Theme.color.textPrimary; font.pointSize: Type.body }
        Text { text: "GPU tier: " + (Graphics.gpuTier ? "yes" : "no"); color: Theme.color.textSecondary; font.pointSize: Type.body }
        Text { text: "Interface scale: " + Graphics.scale; color: Theme.color.textSecondary; font.pointSize: Type.body }
    }
    GraphicsWatch { window: gallery.Window.window }
}
