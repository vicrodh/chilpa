import QtQuick
import app.chilpa.link
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The "Open music link" modal, opened by its button.
Item {
    id: gallery
    implicitWidth: open.implicitWidth + Metrics.contentX * 2 // derived: the sample and its margins
    implicitHeight: open.implicitHeight + Metrics.contentX * 2 // derived: the sample and its margins

    PillButton { id: open; x: Metrics.contentX; y: Metrics.contentX; text: "Open music link"; onClicked: Link.show() }
    LinkModal {}
}
