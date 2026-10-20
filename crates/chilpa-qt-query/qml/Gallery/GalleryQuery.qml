import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Patterns"

Column {
    id: gallery
    property string sample: "genre:jazz,soul -label:\"blue note\" year:1955..1965"
    width: Metrics.menuMaxWidth * 2 // derived: room for rows and the picker
    spacing: Metrics.gap12
    QueryChipRow { query: gallery.sample; onEdited: (query) => gallery.sample = query }
    FocusSwitch {}
    FocusPanel { width: gallery.width; height: Metrics.panesHeight }
    FocusArea { width: gallery.width; height: Metrics.panesHeight; focusMode: true }
    FocusCondition {
        row: ({ field: "genre", label: "Genre", include: ["Jazz"], exclude: ["Pop"], range: null, within: null, minimum: null, missing: false })
    }
    FocusValue { text: "Jazz"; excluded: true }
    FocusValues { field: "genre"; width: gallery.width; height: Metrics.panesHeight }
    FocusYears { width: gallery.width; range: [1960, 1979] }
}
