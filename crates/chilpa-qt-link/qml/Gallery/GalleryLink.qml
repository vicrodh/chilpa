import QtQuick
import app.chilpa.link
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The "Open music link" modal, opened by its button.
Item {
    id: gallery
    implicitWidth: Metrics.linkModal + Metrics.contentX * 2 // derived: the sample and its margins
    implicitHeight: samples.implicitHeight + Metrics.contentX * 2 // derived: the samples and their margins

    Column {
        id: samples
        x: Metrics.contentX; y: Metrics.contentX
        width: Metrics.linkModal
        spacing: Metrics.gap12
        PillButton { text: "Open music link"; onClicked: Link.show() }
        LinkDestination { width: parent.width; destination: ({source:"Library",title:"Example record",artist:"Example artist",local:true}); onChosen: Link.show() }
        LinkDestination { width: parent.width; destination: ({source:"Connected source",title:"Example record",artist:"Example artist"}); onChosen: Link.show() }
    }
    LinkModal {}
}
