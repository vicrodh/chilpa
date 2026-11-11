import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Theme"

// What is typed in a column's header narrows the library as a condition
// does, so it stands on the crumb line as one (design/04 §6): a crumb per
// column, with the columns hidden or Refine open as well, whose x clears
// that header and its box.
Row {
    id: crumbs
    readonly property var searches: RefineText.columnSearches(Library.queries)
    // The width they share; each value gives way to fit.
    property real room: Number.POSITIVE_INFINITY
    visible: searches.length > 0
    spacing: Refine.chipsApart

    Repeater {
        model: crumbs.searches

        FilterCrumb {
            required property var modelData
            objectName: "columnSearchCrumb"
            anchors.verticalCenter: parent.verticalCenter
            name: RefineText.field(modelData.column)
            value: "“" + modelData.text + "”"
            room: (crumbs.room - crumbs.spacing * (crumbs.searches.length - 1)) / crumbs.searches.length // derived: an equal share of the line
            onRemoved: Library.searchColumn(modelData.column, "")
        }
    }
}
