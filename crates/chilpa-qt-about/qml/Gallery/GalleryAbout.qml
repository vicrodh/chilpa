import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The About window, opened by its button, and its body at a card's width.
Item {
    id: gallery
    implicitWidth: column.implicitWidth + Metrics.contentX * 2 // derived: the samples and their margins
    implicitHeight: column.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins
    readonly property int sampleWide: 640

    Column {
        id: column
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.gap16
        PillButton { text: "About"; onClicked: about.open() }
        AboutBody { width: gallery.sampleWide; doc: ({ version: "0.0.0", build: "2026-10-09 (405e25a54)", platform: "Linux (Qt)", links: [{ label: "Website", url: "https://chilpa.app" }] }) }
        UpdatesPanel { width: gallery.sampleWide }
        PluginUpdatesPanel { width: gallery.sampleWide }
        UpdatesIndicator {}
        UpdateNotice {}
    }
    AboutModal { id: about }
}
