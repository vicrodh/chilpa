import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The menu of a cover the listener chose and of an artist's found picture,
// and a picture shown whole, each opened by its button.
Item {
    id: gallery
    implicitWidth: row.implicitWidth + Metrics.contentX * 2 // derived: the buttons and their margins
    implicitHeight: row.implicitHeight + Metrics.contentX * 2 // derived: the buttons and their margins

    Row {
        id: row
        x: Metrics.contentX
        y: Metrics.contentX
        spacing: Metrics.gap12
        PillButton { text: "Chosen cover"; onClicked: chosen.popupAt(0, height) }
        PillButton { text: "Found portrait"; onClicked: found.popupAt(0, height) }
        PillButton { text: "Lightbox"; onClicked: lightbox.show("qrc:/qt/qml/app/chilpa/kit/assets/logo.png") }
    }
    CoverMenu { id: chosen; custom: true; showing: true; address: "https://example.org/cover.jpg" }
    CoverMenu { id: found; kind: "artist"; showing: true }
    CoverLightbox { id: lightbox }
}
