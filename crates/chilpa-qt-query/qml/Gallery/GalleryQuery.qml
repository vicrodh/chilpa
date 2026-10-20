import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"

// The Library's conditions as chips, editing a sample query of their own.
Item {
    id: gallery
    property string sample: "genre:jazz,soul -label:\"blue note\" year:1955..1965"
    implicitWidth: chips.implicitWidth + Metrics.contentX * 2 // derived: the sample and its margins
    implicitHeight: Metrics.crumbHeight + Metrics.contentX * 2 // derived: the sample and its margins

    QueryChipRow {
        id: chips
        x: Metrics.contentX; y: Metrics.contentX
        query: gallery.sample
        onEdited: (query) => gallery.sample = query
    }
}
